#include <math.h>
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#if defined(__GLIBC__)
#include <malloc.h>
#endif

#define GILDER_MONITOR_BANDS 32
#define GILDER_MONITOR_PACKED_WORDS 16
#define GILDER_MONITOR_CHANNELS 2
#define GILDER_MONITOR_RATE 48000
#define GILDER_MONITOR_FORMAT_BUFFER_BYTES 1024
#define GILDER_MONITOR_THREAD_STACK_SIZE "131072"
#define GILDER_MONITOR_PI 3.14159265358979323846

typedef struct GilderSystemAudioMonitor {
    struct pw_thread_loop *loop;
    struct pw_stream *stream;
    atomic_int stream_state;
    atomic_int startup_error;
    atomic_uint_fast64_t process_callbacks;
    atomic_uint spectrum32_packed[GILDER_MONITOR_PACKED_WORDS];
    int loop_started;
} GilderSystemAudioMonitor;

static uint32_t gilder_monitor_quantize(double value) {
    if (!isfinite(value) || value <= 0.0)
        return 0;
    if (value >= 1.0)
        return UINT16_MAX;
    return (uint32_t)(value * (double)UINT16_MAX + 0.5);
}

static double gilder_monitor_mono_sample(const int16_t *samples, int frame) {
    int base = frame * GILDER_MONITOR_CHANNELS;
    return ((double)samples[base] + (double)samples[base + 1]) / 65536.0;
}

static void gilder_monitor_analyze(
    GilderSystemAudioMonitor *monitor,
    const int16_t *samples,
    int frame_count
) {
    if (frame_count < 4)
        return;
    int max_bin = frame_count / 2 - 1;
    if (max_bin < 1)
        return;
    uint32_t bands[GILDER_MONITOR_BANDS];
    for (int band = 0; band < GILDER_MONITOR_BANDS; ++band) {
        int bin = 1 + (band * max_bin) / GILDER_MONITOR_BANDS;
        double omega = 2.0 * GILDER_MONITOR_PI * (double)bin / (double)frame_count;
        double coeff = 2.0 * cos(omega);
        double q0 = 0.0;
        double q1 = 0.0;
        double q2 = 0.0;
        for (int frame = 0; frame < frame_count; ++frame) {
            q0 = coeff * q1 - q2 + gilder_monitor_mono_sample(samples, frame);
            q2 = q1;
            q1 = q0;
        }
        double power = q1 * q1 + q2 * q2 - coeff * q1 * q2;
        double magnitude = power > 0.0 ? sqrt(power) * 2.0 / (double)frame_count : 0.0;
        bands[band] = gilder_monitor_quantize(magnitude);
    }
    for (int word = 0; word < GILDER_MONITOR_PACKED_WORDS; ++word) {
        uint32_t measured = bands[word * 2] | (bands[word * 2 + 1] << 16);
        uint32_t previous = atomic_load_explicit(
            &monitor->spectrum32_packed[word], memory_order_relaxed
        );
        uint32_t low = ((previous & UINT16_MAX) * 13 + (measured & UINT16_MAX) * 7) / 20;
        uint32_t high = (((previous >> 16) * 13 + (measured >> 16) * 7) / 20) << 16;
        atomic_store_explicit(
            &monitor->spectrum32_packed[word], low | high, memory_order_relaxed
        );
    }
}

static void gilder_monitor_state_changed(
    void *data,
    enum pw_stream_state old,
    enum pw_stream_state state,
    const char *error
) {
    (void)old;
    (void)error;
    GilderSystemAudioMonitor *monitor = data;
    atomic_store_explicit(&monitor->stream_state, state, memory_order_release);
    if (state == PW_STREAM_STATE_ERROR)
        atomic_store_explicit(&monitor->startup_error, 1, memory_order_release);
}

static void gilder_monitor_process(void *data) {
    GilderSystemAudioMonitor *monitor = data;
    struct pw_buffer *buffer = pw_stream_dequeue_buffer(monitor->stream);
    if (!buffer)
        return;
    struct spa_buffer *spa_buffer = buffer->buffer;
    if (spa_buffer && spa_buffer->n_datas > 0) {
        struct spa_data *source = &spa_buffer->datas[0];
        if (source->data && source->chunk) {
            uint32_t offset = source->chunk->offset;
            uint32_t size = source->chunk->size;
            if (offset <= source->maxsize && size <= source->maxsize - offset) {
                const int16_t *samples = (const int16_t *)((const uint8_t *)source->data + offset);
                int frame_count = (int)(size / (sizeof(int16_t) * GILDER_MONITOR_CHANNELS));
                gilder_monitor_analyze(monitor, samples, frame_count);
                atomic_fetch_add_explicit(
                    &monitor->process_callbacks, 1, memory_order_relaxed
                );
            }
        }
    }
    pw_stream_queue_buffer(monitor->stream, buffer);
}

static const struct pw_stream_events gilder_monitor_events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = gilder_monitor_state_changed,
    .process = gilder_monitor_process,
};

GilderSystemAudioMonitor *gilder_system_audio_monitor_alloc(void) {
#if defined(__GLIBC__)
    (void)mallopt(M_ARENA_MAX, 1);
    (void)mallopt(M_TRIM_THRESHOLD, 0);
    (void)mallopt(M_TOP_PAD, 0);
#endif
    GilderSystemAudioMonitor *monitor = calloc(1, sizeof(*monitor));
    if (!monitor)
        return NULL;
    pw_init(NULL, NULL);
    const struct spa_dict_item loop_items[] = {
        { SPA_KEY_THREAD_STACK_SIZE, GILDER_MONITOR_THREAD_STACK_SIZE },
    };
    const struct spa_dict loop_properties = SPA_DICT_INIT_ARRAY(loop_items);
    monitor->loop = pw_thread_loop_new(
        "gilder-system-audio-monitor", &loop_properties
    );
    if (!monitor->loop)
        goto fail;
    uint8_t format_buffer[GILDER_MONITOR_FORMAT_BUFFER_BYTES];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(format_buffer, sizeof(format_buffer));
    struct spa_audio_info_raw audio_info = {
        .format = SPA_AUDIO_FORMAT_S16_LE,
        .flags = SPA_AUDIO_FLAG_UNPOSITIONED,
        .rate = GILDER_MONITOR_RATE,
        .channels = GILDER_MONITOR_CHANNELS,
    };
    const struct spa_pod *params[] = {
        spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &audio_info),
    };
    if (!params[0])
        goto fail;
    monitor->stream = pw_stream_new_simple(
        pw_thread_loop_get_loop(monitor->loop),
        "Gilder System Audio Monitor",
        pw_properties_new(
            PW_KEY_MEDIA_TYPE, "Audio",
            PW_KEY_MEDIA_CATEGORY, "Capture",
            PW_KEY_MEDIA_ROLE, "Music",
            PW_KEY_MEDIA_NAME, "Gilder Scene Audio Spectrum",
            PW_KEY_NODE_NAME, "gilder-scene-system-audio-monitor",
            PW_KEY_STREAM_MONITOR, "true",
            PW_KEY_STREAM_CAPTURE_SINK, "true",
            NULL
        ),
        &gilder_monitor_events,
        monitor
    );
    if (!monitor->stream)
        goto fail;
    pw_thread_loop_lock(monitor->loop);
    int result = pw_stream_connect(
        monitor->stream,
        PW_DIRECTION_INPUT,
        PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS,
        params,
        1
    );
    if (result >= 0)
        result = pw_thread_loop_start(monitor->loop);
    if (result >= 0)
        monitor->loop_started = 1;
    pw_thread_loop_unlock(monitor->loop);
    if (result < 0)
        goto fail;
    return monitor;

fail:
    if (monitor->stream)
        pw_stream_destroy(monitor->stream);
    if (monitor->loop_started)
        pw_thread_loop_stop(monitor->loop);
    if (monitor->loop)
        pw_thread_loop_destroy(monitor->loop);
    free(monitor);
    return NULL;
}

void gilder_system_audio_monitor_free(GilderSystemAudioMonitor **handle) {
    if (!handle || !*handle)
        return;
    GilderSystemAudioMonitor *monitor = *handle;
    if (monitor->loop && monitor->stream) {
        pw_thread_loop_lock(monitor->loop);
        pw_stream_destroy(monitor->stream);
        monitor->stream = NULL;
        pw_thread_loop_unlock(monitor->loop);
    }
    if (monitor->loop_started)
        pw_thread_loop_stop(monitor->loop);
    if (monitor->loop)
        pw_thread_loop_destroy(monitor->loop);
    free(monitor);
    *handle = NULL;
}

int gilder_system_audio_monitor_snapshot(
    const GilderSystemAudioMonitor *monitor,
    uint32_t *spectrum32_packed,
    int *stream_state,
    uint64_t *process_callbacks
) {
    if (!monitor || !spectrum32_packed)
        return -1;
    for (int word = 0; word < GILDER_MONITOR_PACKED_WORDS; ++word) {
        spectrum32_packed[word] = atomic_load_explicit(
            &monitor->spectrum32_packed[word], memory_order_relaxed
        );
    }
    uint64_t callbacks = atomic_load_explicit(
        &monitor->process_callbacks, memory_order_acquire
    );
    if (stream_state)
        *stream_state = atomic_load_explicit(&monitor->stream_state, memory_order_acquire);
    if (process_callbacks)
        *process_callbacks = callbacks;
    return callbacks > 0 ? 1 : 0;
}
