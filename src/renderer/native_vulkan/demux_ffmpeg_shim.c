#include <errno.h>
#include <stdint.h>
#include <stddef.h>

#include <libavcodec/avcodec.h>
#include <libavcodec/bsf.h>
#include <libavcodec/packet.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/error.h>

int gilder_av_error_eagain(void) {
    return AVERROR(EAGAIN);
}

int gilder_av_error_eof(void) {
    return AVERROR_EOF;
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

int gilder_avformat_find_stream_info(AVFormatContext *ctx) {
    return avformat_find_stream_info(ctx, NULL);
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

int gilder_av_stream_width(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->codecpar->width;
}

int gilder_av_stream_height(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->codecpar->height;
}

AVRational gilder_av_stream_time_base(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->time_base;
}

AVRational gilder_av_stream_avg_frame_rate(AVFormatContext *ctx, int stream_index) {
    return ctx->streams[stream_index]->avg_frame_rate;
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

int gilder_av_bsf_alloc_name(const char *name, AVBSFContext **ctx) {
    if (!name || !name[0])
        return av_bsf_get_null_filter(ctx);

    const AVBitStreamFilter *filter = av_bsf_get_by_name(name);
    if (!filter)
        return AVERROR(EINVAL);
    return av_bsf_alloc(filter, ctx);
}

int gilder_av_bsf_copy_stream_params(AVBSFContext *bsf, AVFormatContext *fmt, int stream_index) {
    int ret = avcodec_parameters_copy(bsf->par_in, fmt->streams[stream_index]->codecpar);
    if (ret < 0)
        return ret;
    bsf->time_base_in = fmt->streams[stream_index]->time_base;
    return 0;
}

int gilder_av_bsf_init(AVBSFContext *ctx) {
    return av_bsf_init(ctx);
}

void gilder_av_bsf_free(AVBSFContext **ctx) {
    av_bsf_free(ctx);
}

void gilder_av_bsf_flush(AVBSFContext *ctx) {
    av_bsf_flush(ctx);
}

int gilder_av_bsf_send_packet(AVBSFContext *ctx, AVPacket *packet) {
    return av_bsf_send_packet(ctx, packet);
}

int gilder_av_bsf_receive_packet(AVBSFContext *ctx, AVPacket *packet) {
    return av_bsf_receive_packet(ctx, packet);
}

AVRational gilder_av_bsf_time_base_out(AVBSFContext *ctx) {
    return ctx->time_base_out;
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
