#define _GNU_SOURCE

#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <stddef.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/buffer.h>
#include <libavutil/dict.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vulkan.h>
#include <libavutil/mem.h>

#define VR_STREAMING_PROBESIZE_BYTES 32768

typedef struct VRFfmpegVulkanDeviceLease {
    char **instance_extension_storage;
    const char **instance_extensions;
    int instance_extension_count;
    char **device_extension_storage;
    const char **device_extensions;
    int device_extension_count;
    void *vulkan_library;
} VRFfmpegVulkanDeviceLease;

typedef struct VRFfmpegVulkanFrameSnapshot {
    uint64_t image;
    uint64_t semaphore;
    uint64_t semaphore_value;
    uint32_t queue_family;
    uint32_t extra_image_usage;
    uint32_t image_flags;
    int32_t layout;
    int32_t frame_format;
    int32_t software_format;
    int32_t picture_format;
    int32_t width;
    int32_t height;
    int32_t array_layers;
    int32_t image_count;
    int32_t semaphore_count;
    int64_t pts;
    int64_t duration;
} VRFfmpegVulkanFrameSnapshot;

static void vr_free_extensions(char **storage, const char **pointers, int count) {
    if (storage) {
        for (int index = 0; index < count; index++)
            av_free(storage[index]);
    }
    av_free(storage);
    av_free((void *)pointers);
}

static int vr_copy_extensions(
    const char * const *source,
    int count,
    char ***storage_out,
    const char ***pointers_out
) {
    *storage_out = NULL;
    *pointers_out = NULL;
    if (count <= 0)
        return 0;
    if (!source)
        return AVERROR(EINVAL);
    char **storage = av_calloc((size_t)count, sizeof(*storage));
    const char **pointers = av_calloc((size_t)count, sizeof(*pointers));
    if (!storage || !pointers) {
        av_free(storage);
        av_free((void *)pointers);
        return AVERROR(ENOMEM);
    }
    for (int index = 0; index < count; index++) {
        if (!source[index]) {
            vr_free_extensions(storage, pointers, count);
            return AVERROR(EINVAL);
        }
        storage[index] = av_strdup(source[index]);
        if (!storage[index]) {
            vr_free_extensions(storage, pointers, count);
            return AVERROR(ENOMEM);
        }
        pointers[index] = storage[index];
    }
    *storage_out = storage;
    *pointers_out = pointers;
    return 0;
}

static void vr_device_lease_free(VRFfmpegVulkanDeviceLease *lease) {
    if (!lease)
        return;
    vr_free_extensions(
        lease->instance_extension_storage,
        lease->instance_extensions,
        lease->instance_extension_count
    );
    vr_free_extensions(
        lease->device_extension_storage,
        lease->device_extensions,
        lease->device_extension_count
    );
    if (lease->vulkan_library)
        dlclose(lease->vulkan_library);
    av_free(lease);
}

static PFN_vkGetInstanceProcAddr vr_get_instance_proc_addr(
    VRFfmpegVulkanDeviceLease *lease
) {
    PFN_vkGetInstanceProcAddr address =
        (PFN_vkGetInstanceProcAddr)dlsym(RTLD_DEFAULT, "vkGetInstanceProcAddr");
    if (address)
        return address;
    static const char *names[] = {"libvulkan.so.1", "libvulkan.so"};
    for (size_t index = 0; index < sizeof(names) / sizeof(names[0]); index++) {
        void *library = dlopen(names[index], RTLD_NOW | RTLD_LOCAL);
        if (!library)
            continue;
        address = (PFN_vkGetInstanceProcAddr)dlsym(library, "vkGetInstanceProcAddr");
        if (address) {
            lease->vulkan_library = library;
            return address;
        }
        dlclose(library);
    }
    return NULL;
}

static void vr_hwdevice_free(AVHWDeviceContext *context) {
    if (!context)
        return;
    VRFfmpegVulkanDeviceLease *lease = context->user_opaque;
    context->user_opaque = NULL;
    vr_device_lease_free(lease);
}

static enum AVPixelFormat vr_vulkan_format(
    AVCodecContext *context,
    const enum AVPixelFormat *formats
) {
    (void)context;
    for (const enum AVPixelFormat *format = formats;
         *format != AV_PIX_FMT_NONE;
         format++) {
        if (*format == AV_PIX_FMT_VULKAN)
            return AV_PIX_FMT_VULKAN;
    }
    return AV_PIX_FMT_NONE;
}

int vr_ffmpeg_create_vulkan_device(
    AVBufferRef **output,
    uintptr_t instance,
    uintptr_t physical_device,
    uintptr_t device,
    const char * const *instance_extensions,
    int instance_extension_count,
    const char * const *device_extensions,
    int device_extension_count,
    int video_queue_family,
    int video_queue_count,
    uint32_t video_queue_flags,
    uint32_t video_codec_operations
) {
    if (!output || !instance || !physical_device || !device ||
        video_queue_family < 0 || video_queue_count <= 0 ||
        video_codec_operations == 0)
        return AVERROR(EINVAL);
    *output = NULL;
    AVBufferRef *reference = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_VULKAN);
    if (!reference)
        return AVERROR(ENOMEM);
    VRFfmpegVulkanDeviceLease *lease = av_mallocz(sizeof(*lease));
    if (!lease) {
        av_buffer_unref(&reference);
        return AVERROR(ENOMEM);
    }
    int result = vr_copy_extensions(
        instance_extensions,
        instance_extension_count,
        &lease->instance_extension_storage,
        &lease->instance_extensions
    );
    if (result < 0)
        goto fail;
    lease->instance_extension_count = instance_extension_count;
    result = vr_copy_extensions(
        device_extensions,
        device_extension_count,
        &lease->device_extension_storage,
        &lease->device_extensions
    );
    if (result < 0)
        goto fail;
    lease->device_extension_count = device_extension_count;
    AVHWDeviceContext *device_context = (AVHWDeviceContext *)reference->data;
    AVVulkanDeviceContext *vulkan = (AVVulkanDeviceContext *)device_context->hwctx;
    PFN_vkGetInstanceProcAddr get_proc_addr = vr_get_instance_proc_addr(lease);
    if (!get_proc_addr) {
        result = AVERROR(ENOSYS);
        goto fail;
    }
    device_context->user_opaque = lease;
    device_context->free = vr_hwdevice_free;
    vulkan->get_proc_addr = get_proc_addr;
    vulkan->inst = (VkInstance)instance;
    vulkan->phys_dev = (VkPhysicalDevice)physical_device;
    vulkan->act_dev = (VkDevice)device;
    vulkan->enabled_inst_extensions = lease->instance_extensions;
    vulkan->nb_enabled_inst_extensions = lease->instance_extension_count;
    vulkan->enabled_dev_extensions = lease->device_extensions;
    vulkan->nb_enabled_dev_extensions = lease->device_extension_count;
    vulkan->qf[0].idx = video_queue_family;
    vulkan->qf[0].num = video_queue_count;
    vulkan->qf[0].flags = (VkQueueFlagBits)(
        video_queue_flags | VK_QUEUE_VIDEO_DECODE_BIT_KHR
    );
    vulkan->qf[0].video_caps =
        (VkVideoCodecOperationFlagBitsKHR)video_codec_operations;
    vulkan->nb_qf = 1;
    result = av_hwdevice_ctx_init(reference);
    if (result < 0) {
        av_buffer_unref(&reference);
        return result;
    }
    *output = reference;
    return 0;

fail:
    vr_device_lease_free(lease);
    av_buffer_unref(&reference);
    return result;
}

void vr_ffmpeg_buffer_unref(AVBufferRef **reference) {
    av_buffer_unref(reference);
}

int vr_ffmpeg_error_again(void) { return AVERROR(EAGAIN); }
int vr_ffmpeg_error_eof(void) { return AVERROR_EOF; }
int vr_ffmpeg_codec_h264(void) { return AV_CODEC_ID_H264; }
int vr_ffmpeg_codec_hevc(void) { return AV_CODEC_ID_HEVC; }
int vr_ffmpeg_codec_av1(void) { return AV_CODEC_ID_AV1; }
int vr_ffmpeg_pixel_vulkan(void) { return AV_PIX_FMT_VULKAN; }
int vr_ffmpeg_pixel_nv12(void) { return AV_PIX_FMT_NV12; }
int vr_ffmpeg_pixel_p010(void) { return AV_PIX_FMT_P010LE; }
int64_t vr_ffmpeg_nopts_value(void) { return AV_NOPTS_VALUE; }

int vr_ffmpeg_error_string(int error, char *buffer, size_t size) {
    return av_strerror(error, buffer, size);
}

int vr_ffmpeg_open_input(AVFormatContext **context, const char *path) {
    if (!context || !path)
        return AVERROR(EINVAL);
    AVFormatContext *allocated = avformat_alloc_context();
    if (!allocated)
        return AVERROR(ENOMEM);
    allocated->flags |= AVFMT_FLAG_NOBUFFER | AVFMT_FLAG_IGNIDX | AVFMT_FLAG_FAST_SEEK;
    allocated->probesize = VR_STREAMING_PROBESIZE_BYTES;
    allocated->format_probesize = VR_STREAMING_PROBESIZE_BYTES;
    allocated->duration_probesize = 0;
    allocated->max_analyze_duration = 0;
    allocated->fps_probe_size = 0;
    *context = allocated;
    return avformat_open_input(context, path, NULL, NULL);
}

void vr_ffmpeg_close_input(AVFormatContext **context) {
    avformat_close_input(context);
}

int vr_ffmpeg_find_video_stream(AVFormatContext *context, int codec_id) {
    int best = av_find_best_stream(context, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (best >= 0 && context->streams[best]->codecpar->codec_id == codec_id)
        return best;
    for (unsigned int index = 0; index < context->nb_streams; index++) {
        AVStream *stream = context->streams[index];
        if (stream->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
            stream->codecpar->codec_id == codec_id)
            return (int)index;
    }
    return best < 0 ? best : AVERROR_STREAM_NOT_FOUND;
}

AVRational vr_ffmpeg_stream_time_base(AVFormatContext *context, int stream) {
    return context->streams[stream]->time_base;
}

int vr_ffmpeg_seek_start(AVFormatContext *context, int stream) {
    int64_t start = context->streams[stream]->start_time;
    if (start == AV_NOPTS_VALUE)
        start = 0;
    int result = av_seek_frame(context, stream, start, AVSEEK_FLAG_BACKWARD);
    if (result < 0)
        result = av_seek_frame(context, -1, 0, AVSEEK_FLAG_BACKWARD);
    if (result >= 0)
        avformat_flush(context);
    return result;
}

const AVCodec *vr_ffmpeg_stream_decoder(AVFormatContext *context, int stream) {
    enum AVCodecID id = context->streams[stream]->codecpar->codec_id;
    const char *name = id == AV_CODEC_ID_H264 ? "h264" :
                       id == AV_CODEC_ID_HEVC ? "hevc" :
                       id == AV_CODEC_ID_AV1 ? "av1" : NULL;
    if (name) {
        const AVCodec *codec = avcodec_find_decoder_by_name(name);
        if (codec)
            return codec;
    }
    return avcodec_find_decoder(id);
}

const char *vr_ffmpeg_codec_name(const AVCodec *codec) {
    return codec && codec->name ? codec->name : "";
}

int vr_ffmpeg_codec_has_vulkan(const AVCodec *codec) {
    if (!codec)
        return 0;
    for (int index = 0;; index++) {
        const AVCodecHWConfig *config = avcodec_get_hw_config(codec, index);
        if (!config)
            return 0;
        if (config->device_type == AV_HWDEVICE_TYPE_VULKAN &&
            config->pix_fmt == AV_PIX_FMT_VULKAN &&
            (config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX))
            return 1;
    }
}

AVCodecContext *vr_ffmpeg_codec_alloc(const AVCodec *codec) {
    return avcodec_alloc_context3(codec);
}

void vr_ffmpeg_codec_free(AVCodecContext **context) {
    avcodec_free_context(context);
}

int vr_ffmpeg_codec_copy_parameters(
    AVCodecContext *codec,
    AVFormatContext *format,
    int stream
) {
    return avcodec_parameters_to_context(codec, format->streams[stream]->codecpar);
}

int vr_ffmpeg_codec_open(
    AVCodecContext *context,
    const AVCodec *codec,
    AVBufferRef *device
) {
    if (!context || !codec || !device)
        return AVERROR(EINVAL);
    context->thread_count = 1;
    context->thread_type = 0;
    context->extra_hw_frames = 0;
    context->flags |= AV_CODEC_FLAG_LOW_DELAY;
    context->flags2 |= AV_CODEC_FLAG2_FAST;
    context->get_format = vr_vulkan_format;
    context->hw_device_ctx = av_buffer_ref(device);
    if (!context->hw_device_ctx)
        return AVERROR(ENOMEM);
    AVDictionary *options = NULL;
    if (codec->id == AV_CODEC_ID_H264) {
        int option_result = av_dict_set(&options, "enable_er", "0", 0);
        if (option_result < 0) {
            av_buffer_unref(&context->hw_device_ctx);
            return option_result;
        }
    }
    int result = avcodec_open2(context, codec, &options);
    av_dict_free(&options);
    if (result < 0)
        av_buffer_unref(&context->hw_device_ctx);
    return result;
}

int vr_ffmpeg_codec_send(AVCodecContext *context, const AVPacket *packet) {
    return avcodec_send_packet(context, packet);
}

int vr_ffmpeg_codec_receive(AVCodecContext *context, AVFrame *frame) {
    return avcodec_receive_frame(context, frame);
}

void vr_ffmpeg_codec_flush(AVCodecContext *context) {
    avcodec_flush_buffers(context);
}

int vr_ffmpeg_codec_width(const AVCodecContext *context) {
    return context ? context->coded_width : 0;
}

int vr_ffmpeg_codec_height(const AVCodecContext *context) {
    return context ? context->coded_height : 0;
}

AVPacket *vr_ffmpeg_packet_alloc(void) { return av_packet_alloc(); }
void vr_ffmpeg_packet_free(AVPacket **packet) { av_packet_free(packet); }
void vr_ffmpeg_packet_unref(AVPacket *packet) { av_packet_unref(packet); }
int vr_ffmpeg_read(AVFormatContext *context, AVPacket *packet) {
    return av_read_frame(context, packet);
}
int vr_ffmpeg_packet_stream(const AVPacket *packet) { return packet->stream_index; }
int vr_ffmpeg_packet_size(const AVPacket *packet) { return packet->size; }

AVFrame *vr_ffmpeg_frame_alloc(void) { return av_frame_alloc(); }
void vr_ffmpeg_frame_free(AVFrame **frame) { av_frame_free(frame); }
void vr_ffmpeg_frame_unref(AVFrame *frame) { av_frame_unref(frame); }
void vr_ffmpeg_frame_move(AVFrame *destination, AVFrame *source) {
    av_frame_move_ref(destination, source);
}

#if defined(VK_USE_64_BIT_PTR_DEFINES) && VK_USE_64_BIT_PTR_DEFINES == 1
#define VR_HANDLE_U64(handle) ((uint64_t)(uintptr_t)(handle))
#else
#define VR_HANDLE_U64(handle) ((uint64_t)(handle))
#endif

int vr_ffmpeg_frame_snapshot(
    const AVFrame *frame,
    VRFfmpegVulkanFrameSnapshot *snapshot
) {
    if (!frame || !snapshot || frame->format != AV_PIX_FMT_VULKAN ||
        !frame->hw_frames_ctx || !frame->hw_frames_ctx->data || !frame->data[0])
        return AVERROR(EINVAL);
    AVHWFramesContext *frames = (AVHWFramesContext *)frame->hw_frames_ctx->data;
    AVVulkanFramesContext *vulkan_frames = (AVVulkanFramesContext *)frames->hwctx;
    AVVkFrame *vulkan = (AVVkFrame *)frame->data[0];
    if (!vulkan_frames)
        return AVERROR(EINVAL);
    if (vulkan_frames->lock_frame)
        vulkan_frames->lock_frame(frames, vulkan);
    int image_count = 0;
    int semaphore_count = 0;
    for (int index = 0; index < AV_NUM_DATA_POINTERS; index++) {
        if (vulkan->img[index] != VK_NULL_HANDLE)
            image_count++;
        if (vulkan->sem[index] != VK_NULL_HANDLE)
            semaphore_count++;
    }
    snapshot->image = VR_HANDLE_U64(vulkan->img[0]);
    snapshot->semaphore = VR_HANDLE_U64(vulkan->sem[0]);
    snapshot->semaphore_value = vulkan->sem_value[0];
    snapshot->queue_family = vulkan->queue_family[0];
    snapshot->extra_image_usage = vulkan_frames->usage;
    snapshot->image_flags = vulkan_frames->img_flags;
    snapshot->layout = (int32_t)vulkan->layout[0];
    snapshot->frame_format = frame->format;
    snapshot->software_format = frames->sw_format;
    snapshot->picture_format = vulkan_frames->format[0];
    snapshot->width = frame->width;
    snapshot->height = frame->height;
    snapshot->array_layers = vulkan_frames->nb_layers > 0 ? vulkan_frames->nb_layers : 1;
    snapshot->image_count = image_count;
    snapshot->semaphore_count = semaphore_count;
    snapshot->pts = frame->pts;
    snapshot->duration = frame->duration;
    if (vulkan_frames->unlock_frame)
        vulkan_frames->unlock_frame(frames, vulkan);
    return 0;
}

size_t vr_ffmpeg_frame_snapshot_size(void) {
    return sizeof(VRFfmpegVulkanFrameSnapshot);
}
