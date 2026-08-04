#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define TENSOR_WALLPAPER_MONITOR_CHANNELS 2
#define TENSOR_WALLPAPER_MONITOR_RATE 48000
#define TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_FRAMES 4096
#define TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES \
    (TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_FRAMES * TENSOR_WALLPAPER_MONITOR_CHANNELS)
#define TENSOR_WALLPAPER_MONITOR_FORMAT_BUFFER_BYTES 1024
#define TENSOR_WALLPAPER_MONITOR_THREAD_STACK_SIZE "131072"
typedef struct TensorWallpaperSystemAudioMonitor {
    struct pw_thread_loop *loop;
    struct pw_stream *stream;
    float pcm[TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES];
    uint32_t pending_samples;
    atomic_int stream_state;
    atomic_int startup_error;
    atomic_uint_fast64_t process_callbacks;
    int loop_started;
} TensorWallpaperSystemAudioMonitor;

static void tensor_wallpaper_monitor_retain_pcm(
    TensorWallpaperSystemAudioMonitor *monitor,
    const float *samples,
    uint32_t sample_count
) {
    sample_count -= sample_count % TENSOR_WALLPAPER_MONITOR_CHANNELS;
    if (sample_count == 0)
        return;
    if (sample_count >= TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES) {
        const float *tail = samples + sample_count - TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES;
        memcpy(monitor->pcm, tail, sizeof(monitor->pcm));
        monitor->pending_samples = TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES;
        return;
    }
    uint32_t total = monitor->pending_samples + sample_count;
    if (total > TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES) {
        uint32_t discarded = total - TENSOR_WALLPAPER_MONITOR_PCM_CAPACITY_SAMPLES;
        memmove(
            monitor->pcm,
            monitor->pcm + discarded,
            (monitor->pending_samples - discarded) * sizeof(float)
        );
        monitor->pending_samples -= discarded;
    }
    memcpy(monitor->pcm + monitor->pending_samples, samples, sample_count * sizeof(float));
    monitor->pending_samples += sample_count;
}

static void tensor_wallpaper_monitor_state_changed(
    void *data,
    enum pw_stream_state old,
    enum pw_stream_state state,
    const char *error
) {
    (void)old;
    (void)error;
    TensorWallpaperSystemAudioMonitor *monitor = data;
    atomic_store_explicit(&monitor->stream_state, state, memory_order_release);
    if (state == PW_STREAM_STATE_ERROR)
        atomic_store_explicit(&monitor->startup_error, 1, memory_order_release);
}

static void tensor_wallpaper_monitor_process(void *data) {
    TensorWallpaperSystemAudioMonitor *monitor = data;
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
                const float *samples = (const float *)((const uint8_t *)source->data + offset);
                uint32_t sample_count = size / sizeof(float);
                tensor_wallpaper_monitor_retain_pcm(monitor, samples, sample_count);
                atomic_fetch_add_explicit(
                    &monitor->process_callbacks, 1, memory_order_relaxed
                );
            }
        }
    }
    pw_stream_queue_buffer(monitor->stream, buffer);
}

static const struct pw_stream_events tensor_wallpaper_monitor_events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = tensor_wallpaper_monitor_state_changed,
    .process = tensor_wallpaper_monitor_process,
};

TensorWallpaperSystemAudioMonitor *tensor_wallpaper_system_audio_monitor_alloc(void) {
    TensorWallpaperSystemAudioMonitor *monitor = calloc(1, sizeof(*monitor));
    if (!monitor)
        return NULL;
    pw_init(NULL, NULL);
    const struct spa_dict_item loop_items[] = {
        { SPA_KEY_THREAD_STACK_SIZE, TENSOR_WALLPAPER_MONITOR_THREAD_STACK_SIZE },
    };
    const struct spa_dict loop_properties = SPA_DICT_INIT_ARRAY(loop_items);
    monitor->loop = pw_thread_loop_new(
        "tensor-wallpaper-system-audio-monitor", &loop_properties
    );
    if (!monitor->loop)
        goto fail;
    uint8_t format_buffer[TENSOR_WALLPAPER_MONITOR_FORMAT_BUFFER_BYTES];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(format_buffer, sizeof(format_buffer));
    struct spa_audio_info_raw audio_info = {
        .format = SPA_AUDIO_FORMAT_F32_LE,
        .flags = SPA_AUDIO_FLAG_UNPOSITIONED,
        .rate = TENSOR_WALLPAPER_MONITOR_RATE,
        .channels = TENSOR_WALLPAPER_MONITOR_CHANNELS,
    };
    const struct spa_pod *params[] = {
        spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &audio_info),
    };
    if (!params[0])
        goto fail;
    monitor->stream = pw_stream_new_simple(
        pw_thread_loop_get_loop(monitor->loop),
        "Tensor Wallpaper System Audio Monitor",
        pw_properties_new(
            PW_KEY_MEDIA_TYPE, "Audio",
            PW_KEY_MEDIA_CATEGORY, "Capture",
            PW_KEY_MEDIA_ROLE, "Music",
            PW_KEY_MEDIA_NAME, "Tensor Wallpaper Scene Audio Spectrum",
            PW_KEY_NODE_NAME, "tensor-wallpaper-scene-system-audio-monitor",
            PW_KEY_STREAM_MONITOR, "true",
            PW_KEY_STREAM_CAPTURE_SINK, "true",
            NULL
        ),
        &tensor_wallpaper_monitor_events,
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

void tensor_wallpaper_system_audio_monitor_free(TensorWallpaperSystemAudioMonitor **handle) {
    if (!handle || !*handle)
        return;
    TensorWallpaperSystemAudioMonitor *monitor = *handle;
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

int tensor_wallpaper_system_audio_monitor_snapshot(
    TensorWallpaperSystemAudioMonitor *monitor,
    float *pcm,
    uint32_t pcm_capacity,
    uint32_t *sample_count,
    int *stream_state,
    uint64_t *process_callbacks
) {
    if (!monitor || !pcm || !sample_count)
        return -1;
    pw_thread_loop_lock(monitor->loop);
    uint32_t copied = monitor->pending_samples;
    if (copied > pcm_capacity)
        copied = pcm_capacity;
    copied -= copied % TENSOR_WALLPAPER_MONITOR_CHANNELS;
    memcpy(pcm, monitor->pcm, copied * sizeof(float));
    if (copied < monitor->pending_samples) {
        memmove(
            monitor->pcm,
            monitor->pcm + copied,
            (monitor->pending_samples - copied) * sizeof(float)
        );
    }
    monitor->pending_samples -= copied;
    pw_thread_loop_unlock(monitor->loop);
    *sample_count = copied;
    uint64_t callbacks = atomic_load_explicit(
        &monitor->process_callbacks, memory_order_acquire
    );
    if (stream_state)
        *stream_state = atomic_load_explicit(&monitor->stream_state, memory_order_acquire);
    if (process_callbacks)
        *process_callbacks = callbacks;
    return copied > 0 ? 1 : 0;
}
