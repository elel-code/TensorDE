#define _GNU_SOURCE

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavcodec/packet.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/samplefmt.h>
#include <libswresample/swresample.h>
#include <pipewire/pipewire.h>
#include <spa/param/audio/raw-utils.h>
#include <spa/param/param.h>

#define GILDER_AUDIO_PIPEWIRE_CONNECT_TIMEOUT_NS (2LL * 1000LL * 1000LL * 1000LL)
#define GILDER_AUDIO_PIPEWIRE_WRITE_TIMEOUT_NS (2LL * 1000LL * 1000LL * 1000LL)
#define GILDER_AUDIO_PIPEWIRE_FORMAT_BUFFER_BYTES 1024

typedef struct GilderAudioOutput {
    SwrContext *swr;
    struct pw_thread_loop *loop;
    struct pw_stream *stream;
    enum pw_stream_state stream_state;
    int stream_error;
    const uint8_t *pending_data;
    size_t pending_size;
    size_t pending_offset;
    int pending_error;
    bool loop_started;
    int sample_rate;
    int channels;
    int64_t written_samples;
    int64_t written_bytes;
    int64_t write_call_count;
    int64_t write_wait_count;
    int64_t process_callback_count;
    int64_t buffer_error_count;
    int64_t timeout_error_count;
    int64_t state_change_count;
    int64_t ready_state_change_count;
} GilderAudioOutput;

static int gilder_pipewire_initialized = 0;

int gilder_av_error_eof(void) {
    return AVERROR_EOF;
}

int gilder_av_error_again(void) {
    return AVERROR(EAGAIN);
}

int64_t gilder_av_nopts_value(void) {
    return AV_NOPTS_VALUE;
}

int gilder_av_codec_id_h264(void) {
    return AV_CODEC_ID_H264;
}

int gilder_av_codec_id_hevc(void) {
    return AV_CODEC_ID_HEVC;
}

int gilder_av_codec_id_av1(void) {
    return AV_CODEC_ID_AV1;
}

int gilder_av_strerror(int errnum, char *errbuf, size_t errbuf_size) {
    return av_strerror(errnum, errbuf, errbuf_size);
}

int gilder_avformat_open_input(AVFormatContext **ctx, const char *url) {
    return avformat_open_input(ctx, url, NULL, NULL);
}

void gilder_avformat_close_input(AVFormatContext **ctx) {
    avformat_close_input(ctx);
}

int gilder_av_find_video_stream_for_codec(AVFormatContext *ctx, int codec_id) {
    int best = av_find_best_stream(ctx, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (best >= 0 && ctx->streams[best]->codecpar->codec_id == codec_id)
        return best;

    for (unsigned int i = 0; i < ctx->nb_streams; i++) {
        AVStream *stream = ctx->streams[i];
        if (stream->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
            stream->codecpar->codec_id == codec_id)
            return (int)i;
    }

    if (best < 0)
        return best;
    return AVERROR_STREAM_NOT_FOUND;
}

int gilder_av_find_audio_stream(AVFormatContext *ctx) {
    int best = av_find_best_stream(ctx, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
    if (best >= 0)
        return best;

    for (unsigned int i = 0; i < ctx->nb_streams; i++) {
        AVStream *stream = ctx->streams[i];
        if (stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO)
            return (int)i;
    }

    return best;
}

AVPacket *gilder_av_packet_alloc(void) {
    return av_packet_alloc();
}

void gilder_av_packet_free(AVPacket **packet) {
    av_packet_free(packet);
}

void gilder_av_packet_unref(AVPacket *packet) {
    av_packet_unref(packet);
}

int gilder_av_read_frame(AVFormatContext *ctx, AVPacket *packet) {
    return av_read_frame(ctx, packet);
}

int gilder_av_packet_stream_index(const AVPacket *packet) {
    return packet->stream_index;
}

const uint8_t *gilder_av_packet_data(const AVPacket *packet) {
    return packet->data;
}

int gilder_av_packet_size(const AVPacket *packet) {
    return packet->size;
}

int64_t gilder_av_packet_pts(const AVPacket *packet) {
    return packet->pts;
}

int64_t gilder_av_packet_duration(const AVPacket *packet) {
    return packet->duration;
}

const uint8_t *gilder_av_stream_extradata(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->codecpar->extradata;
}

int gilder_av_stream_extradata_size(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->codecpar->extradata_size;
}

AVRational gilder_av_stream_time_base(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->time_base;
}

int gilder_av_seek_stream_start(AVFormatContext *ctx, int stream_index) {
    int64_t start_time = ctx->streams[stream_index]->start_time;
    if (start_time == AV_NOPTS_VALUE)
        start_time = 0;

    int ret = av_seek_frame(ctx, stream_index, start_time, AVSEEK_FLAG_BACKWARD);
    if (ret < 0)
        ret = av_seek_frame(ctx, -1, 0, AVSEEK_FLAG_BACKWARD);
    if (ret >= 0)
        avformat_flush(ctx);
    return ret;
}

const AVCodec *gilder_av_stream_decoder(AVFormatContext *ctx, int stream_index) {
    return avcodec_find_decoder(ctx->streams[stream_index]->codecpar->codec_id);
}

AVCodecContext *gilder_avcodec_alloc_context3(const AVCodec *codec) {
    return avcodec_alloc_context3(codec);
}

void gilder_avcodec_free_context(AVCodecContext **ctx) {
    avcodec_free_context(ctx);
}

int gilder_avcodec_parameters_to_context_for_stream(
    AVCodecContext *codec_ctx,
    AVFormatContext *format_ctx,
    int stream_index
) {
    return avcodec_parameters_to_context(codec_ctx, format_ctx->streams[stream_index]->codecpar);
}

int gilder_avcodec_open2(AVCodecContext *ctx, const AVCodec *codec) {
    return avcodec_open2(ctx, codec, NULL);
}

int gilder_avcodec_send_packet(AVCodecContext *ctx, const AVPacket *packet) {
    return avcodec_send_packet(ctx, packet);
}

int gilder_avcodec_receive_frame(AVCodecContext *ctx, AVFrame *frame) {
    return avcodec_receive_frame(ctx, frame);
}

int gilder_avcodec_context_sample_rate(const AVCodecContext *ctx) {
    return ctx->sample_rate;
}

int gilder_avcodec_context_channels(const AVCodecContext *ctx) {
    return ctx->ch_layout.nb_channels;
}

AVFrame *gilder_av_frame_alloc(void) {
    return av_frame_alloc();
}

void gilder_av_frame_free(AVFrame **frame) {
    av_frame_free(frame);
}

void gilder_av_frame_unref(AVFrame *frame) {
    av_frame_unref(frame);
}

int gilder_av_frame_nb_samples(const AVFrame *frame) {
    return frame->nb_samples;
}

int gilder_av_frame_sample_rate(const AVFrame *frame) {
    return frame->sample_rate;
}

int gilder_av_frame_channels(const AVFrame *frame) {
    return frame->ch_layout.nb_channels;
}

GilderAudioOutput *gilder_audio_output_alloc(void) {
    return av_mallocz(sizeof(GilderAudioOutput));
}

static void gilder_pipewire_init_once(void) {
    if (!gilder_pipewire_initialized) {
        pw_init(NULL, NULL);
        gilder_pipewire_initialized = 1;
    }
}

static int gilder_audio_output_wait_locked(GilderAudioOutput *out, int64_t timeout_ns) {
    struct timespec timeout;
    int ret = pw_thread_loop_get_time(out->loop, &timeout, timeout_ns);
    if (ret < 0)
        return ret;
    ret = pw_thread_loop_timed_wait_full(out->loop, &timeout);
    return ret < 0 ? ret : 0;
}

static int gilder_audio_output_stream_error(const GilderAudioOutput *out) {
    if (out->stream_error < 0)
        return out->stream_error;
    if (out->stream_state == PW_STREAM_STATE_ERROR)
        return AVERROR(EPIPE);
    return 0;
}

static bool gilder_audio_output_stream_ready(const GilderAudioOutput *out) {
    return out->stream_state == PW_STREAM_STATE_PAUSED ||
           out->stream_state == PW_STREAM_STATE_STREAMING;
}

static void gilder_audio_output_on_state_changed(
    void *data,
    enum pw_stream_state old,
    enum pw_stream_state state,
    const char *error
) {
    (void)old;
    (void)error;
    GilderAudioOutput *out = data;
    out->stream_state = state;
    out->state_change_count++;
    if (gilder_audio_output_stream_ready(out))
        out->ready_state_change_count++;
    if (state == PW_STREAM_STATE_ERROR)
        out->stream_error = errno != 0 ? AVERROR(errno) : AVERROR(EPIPE);
    pw_thread_loop_signal(out->loop, false);
}

static void gilder_audio_output_on_process(void *data) {
    GilderAudioOutput *out = data;
    out->process_callback_count++;
    struct pw_buffer *buffer = pw_stream_dequeue_buffer(out->stream);
    if (!buffer) {
        out->buffer_error_count++;
        out->pending_error = AVERROR(EPIPE);
        pw_thread_loop_signal(out->loop, false);
        return;
    }

    struct spa_buffer *spa_buffer = buffer->buffer;
    if (!spa_buffer || spa_buffer->n_datas == 0 || !spa_buffer->datas[0].data ||
        !spa_buffer->datas[0].chunk) {
        pw_stream_return_buffer(out->stream, buffer);
        out->buffer_error_count++;
        out->pending_error = AVERROR(EINVAL);
        pw_thread_loop_signal(out->loop, false);
        return;
    }

    struct spa_data *dst = &spa_buffer->datas[0];
    size_t remaining = 0;
    if (out->pending_data && out->pending_offset < out->pending_size)
        remaining = out->pending_size - out->pending_offset;
    size_t copied = remaining < dst->maxsize ? remaining : dst->maxsize;
    if (copied > 0) {
        memcpy(dst->data, out->pending_data + out->pending_offset, copied);
        out->pending_offset += copied;
    } else if (remaining > 0) {
        out->buffer_error_count++;
        out->pending_error = AVERROR(EPIPE);
    }

    dst->chunk->offset = 0;
    dst->chunk->size = copied > UINT32_MAX ? UINT32_MAX : (uint32_t)copied;
    dst->chunk->stride = out->channels > 0 ? out->channels * (int)sizeof(int16_t) : 0;
    dst->chunk->flags = copied == 0 ? SPA_CHUNK_FLAG_EMPTY : SPA_CHUNK_FLAG_NONE;
    pw_stream_queue_buffer(out->stream, buffer);

    if (copied > 0 || remaining == 0 || out->pending_error < 0)
        pw_thread_loop_signal(out->loop, false);
}

static const struct pw_stream_events gilder_audio_output_stream_events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = gilder_audio_output_on_state_changed,
    .process = gilder_audio_output_on_process,
};

static void gilder_audio_output_destroy_stream(GilderAudioOutput *out) {
    if (out->loop && out->stream) {
        pw_thread_loop_lock(out->loop);
        pw_stream_destroy(out->stream);
        out->stream = NULL;
        pw_thread_loop_unlock(out->loop);
    }
    if (out->loop_started) {
        pw_thread_loop_stop(out->loop);
        out->loop_started = false;
    }
    if (out->loop) {
        pw_thread_loop_destroy(out->loop);
        out->loop = NULL;
    }
    out->stream_state = PW_STREAM_STATE_UNCONNECTED;
    out->stream_error = 0;
    out->pending_data = NULL;
    out->pending_size = 0;
    out->pending_offset = 0;
    out->pending_error = 0;
}

void gilder_audio_output_free(GilderAudioOutput **output) {
    if (!output || !*output)
        return;
    GilderAudioOutput *out = *output;
    gilder_audio_output_destroy_stream(out);
    swr_free(&out->swr);
    av_free(out);
    *output = NULL;
}

static int gilder_audio_output_channel_count(const AVFrame *frame, const AVCodecContext *codec_ctx) {
    int channels = frame->ch_layout.nb_channels;
    if (channels <= 0)
        channels = codec_ctx->ch_layout.nb_channels;
    if (channels <= 0)
        channels = 2;
    if (channels > 8)
        channels = 2;
    return channels;
}

static int gilder_audio_output_sample_rate(const AVFrame *frame, const AVCodecContext *codec_ctx) {
    if (frame->sample_rate > 0)
        return frame->sample_rate;
    if (codec_ctx->sample_rate > 0)
        return codec_ctx->sample_rate;
    return 48000;
}

static int gilder_audio_output_start_stream(GilderAudioOutput *out, int sample_rate, int channels) {
    gilder_pipewire_init_once();
    out->loop = pw_thread_loop_new("gilder-native-vulkan-audio", NULL);
    if (!out->loop)
        return AVERROR(ENOMEM);

    out->stream_state = PW_STREAM_STATE_UNCONNECTED;
    out->stream_error = 0;
    out->stream = pw_stream_new_simple(
        pw_thread_loop_get_loop(out->loop),
        "Gilder Native Vulkan Audio",
        pw_properties_new(
            PW_KEY_MEDIA_TYPE,
            "Audio",
            PW_KEY_MEDIA_CATEGORY,
            "Playback",
            PW_KEY_MEDIA_ROLE,
            "Movie",
            PW_KEY_MEDIA_NAME,
            "Gilder Native Vulkan",
            PW_KEY_NODE_NAME,
            "gilder-native-vulkan-audio",
            NULL
        ),
        &gilder_audio_output_stream_events,
        out
    );
    if (!out->stream) {
        gilder_audio_output_destroy_stream(out);
        return AVERROR(ENOMEM);
    }

    uint8_t buffer[GILDER_AUDIO_PIPEWIRE_FORMAT_BUFFER_BYTES];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));
    struct spa_audio_info_raw audio_info = {
        .format = SPA_AUDIO_FORMAT_S16_LE,
        .flags = SPA_AUDIO_FLAG_UNPOSITIONED,
        .rate = (uint32_t)sample_rate,
        .channels = (uint32_t)channels,
    };
    const struct spa_pod *params[1] = {
        spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &audio_info),
    };
    if (!params[0]) {
        gilder_audio_output_destroy_stream(out);
        return AVERROR(EINVAL);
    }

    pw_thread_loop_lock(out->loop);
    int ret = pw_stream_connect(
        out->stream,
        PW_DIRECTION_OUTPUT,
        PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_EARLY_PROCESS,
        params,
        1
    );
    if (ret < 0) {
        pw_thread_loop_unlock(out->loop);
        gilder_audio_output_destroy_stream(out);
        return ret;
    }

    ret = pw_thread_loop_start(out->loop);
    if (ret < 0) {
        pw_thread_loop_unlock(out->loop);
        gilder_audio_output_destroy_stream(out);
        return ret;
    }
    out->loop_started = true;

    while (out->stream_state != PW_STREAM_STATE_PAUSED &&
           out->stream_state != PW_STREAM_STATE_STREAMING &&
           out->stream_state != PW_STREAM_STATE_ERROR) {
        ret = gilder_audio_output_wait_locked(out, GILDER_AUDIO_PIPEWIRE_CONNECT_TIMEOUT_NS);
        if (ret < 0)
            break;
    }
    int stream_error = gilder_audio_output_stream_error(out);
    pw_thread_loop_unlock(out->loop);
    if (stream_error < 0) {
        gilder_audio_output_destroy_stream(out);
        return stream_error;
    }
    if (ret < 0) {
        gilder_audio_output_destroy_stream(out);
        return ret;
    }
    if (out->stream_state != PW_STREAM_STATE_PAUSED &&
        out->stream_state != PW_STREAM_STATE_STREAMING) {
        gilder_audio_output_destroy_stream(out);
        return AVERROR(ETIMEDOUT);
    }
    return 0;
}

static int gilder_audio_output_ensure_started(
    GilderAudioOutput *out,
    const AVCodecContext *codec_ctx,
    const AVFrame *frame
) {
    int sample_rate = gilder_audio_output_sample_rate(frame, codec_ctx);
    int channels = gilder_audio_output_channel_count(frame, codec_ctx);
    if (out->stream && out->swr && out->sample_rate == sample_rate && out->channels == channels)
        return 0;

    gilder_audio_output_destroy_stream(out);
    swr_free(&out->swr);

    AVChannelLayout out_layout;
    AVChannelLayout in_layout;
    av_channel_layout_default(&out_layout, channels);
    if (frame->ch_layout.nb_channels > 0)
        av_channel_layout_copy(&in_layout, &frame->ch_layout);
    else if (codec_ctx->ch_layout.nb_channels > 0)
        av_channel_layout_copy(&in_layout, &codec_ctx->ch_layout);
    else
        av_channel_layout_default(&in_layout, channels);
    int ret = swr_alloc_set_opts2(
        &out->swr,
        &out_layout,
        AV_SAMPLE_FMT_S16,
        sample_rate,
        &in_layout,
        frame->format,
        sample_rate,
        0,
        NULL
    );
    av_channel_layout_uninit(&out_layout);
    av_channel_layout_uninit(&in_layout);
    if (ret < 0)
        return ret;
    ret = swr_init(out->swr);
    if (ret < 0)
        return ret;

    ret = gilder_audio_output_start_stream(out, sample_rate, channels);
    if (ret < 0)
        return ret;

    out->sample_rate = sample_rate;
    out->channels = channels;
    return 0;
}

static int gilder_audio_output_write_bytes(GilderAudioOutput *out, const uint8_t *data, size_t size) {
    if (size == 0)
        return 0;
    out->write_call_count++;
    pw_thread_loop_lock(out->loop);
    int ret = gilder_audio_output_stream_error(out);
    if (ret < 0) {
        pw_thread_loop_unlock(out->loop);
        return ret;
    }

    out->pending_data = data;
    out->pending_size = size;
    out->pending_offset = 0;
    out->pending_error = 0;

    while (out->pending_offset < out->pending_size && out->pending_error == 0) {
        ret = gilder_audio_output_stream_error(out);
        if (ret < 0)
            break;
        (void)pw_stream_trigger_process(out->stream);
        out->write_wait_count++;
        ret = gilder_audio_output_wait_locked(out, GILDER_AUDIO_PIPEWIRE_WRITE_TIMEOUT_NS);
        if (ret < 0) {
            if (ret == AVERROR(ETIMEDOUT))
                out->timeout_error_count++;
            break;
        }
    }
    if (ret >= 0 && out->pending_error < 0)
        ret = out->pending_error;
    if (ret >= 0 && out->pending_offset < out->pending_size) {
        out->timeout_error_count++;
        ret = AVERROR(ETIMEDOUT);
    }

    out->pending_data = NULL;
    out->pending_size = 0;
    out->pending_offset = 0;
    out->pending_error = 0;
    pw_thread_loop_unlock(out->loop);
    return ret;
}

int gilder_audio_output_write_frame(
    GilderAudioOutput *out,
    AVCodecContext *codec_ctx,
    const AVFrame *frame,
    int64_t *samples_written,
    int64_t *bytes_written,
    int *sample_rate,
    int *channels,
    int64_t *write_calls,
    int64_t *write_waits,
    int64_t *process_callbacks,
    int64_t *buffer_errors,
    int64_t *timeout_errors,
    int *stream_ready,
    int64_t *state_changes,
    int64_t *ready_state_changes,
    int *stream_state
) {
    if (!out || !codec_ctx || !frame)
        return AVERROR(EINVAL);
    int ret = gilder_audio_output_ensure_started(out, codec_ctx, frame);
    if (ret < 0)
        return ret;

    int dst_samples = (int)av_rescale_rnd(
        swr_get_delay(out->swr, out->sample_rate) + frame->nb_samples,
        out->sample_rate,
        out->sample_rate,
        AV_ROUND_UP
    );
    if (dst_samples <= 0)
        return 0;

    uint8_t **dst_data = NULL;
    int dst_linesize = 0;
    ret = av_samples_alloc_array_and_samples(
        &dst_data,
        &dst_linesize,
        out->channels,
        dst_samples,
        AV_SAMPLE_FMT_S16,
        0
    );
    if (ret < 0)
        return ret;

    int converted = swr_convert(
        out->swr,
        dst_data,
        dst_samples,
        (const uint8_t **)frame->extended_data,
        frame->nb_samples
    );
    if (converted < 0) {
        av_freep(&dst_data[0]);
        av_freep(&dst_data);
        return converted;
    }

    int byte_count = av_samples_get_buffer_size(
        NULL,
        out->channels,
        converted,
        AV_SAMPLE_FMT_S16,
        1
    );
    if (byte_count < 0) {
        av_freep(&dst_data[0]);
        av_freep(&dst_data);
        return byte_count;
    }

    ret = gilder_audio_output_write_bytes(out, dst_data[0], (size_t)byte_count);
    if (write_calls)
        *write_calls = out->write_call_count;
    if (write_waits)
        *write_waits = out->write_wait_count;
    if (process_callbacks)
        *process_callbacks = out->process_callback_count;
    if (buffer_errors)
        *buffer_errors = out->buffer_error_count;
    if (timeout_errors)
        *timeout_errors = out->timeout_error_count;
    if (stream_ready)
        *stream_ready = gilder_audio_output_stream_ready(out) ? 1 : 0;
    if (state_changes)
        *state_changes = out->state_change_count;
    if (ready_state_changes)
        *ready_state_changes = out->ready_state_change_count;
    if (stream_state)
        *stream_state = (int)out->stream_state;
    av_freep(&dst_data[0]);
    av_freep(&dst_data);
    if (ret < 0)
        return ret;

    out->written_samples += converted;
    out->written_bytes += byte_count;
    if (samples_written)
        *samples_written = converted;
    if (bytes_written)
        *bytes_written = byte_count;
    if (sample_rate)
        *sample_rate = out->sample_rate;
    if (channels)
        *channels = out->channels;
    return 0;
}
