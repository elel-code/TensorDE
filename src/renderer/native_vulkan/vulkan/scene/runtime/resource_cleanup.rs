use super::*;

pub(super) fn destroy_scene_present_runtime_resources(
    device: &Device,
    frame_contexts: Vec<ScenePresentFrameContext>,
    render_finished: Vec<vk::Semaphore>,
    swapchain_views: Vec<vk::ImageView>,
    frame_capture: Option<SceneFrameCapture>,
    gpu_timing: Option<SceneGpuTiming>,
    scene_resources: SceneGpuResources,
    command_pool: vk::CommandPool,
    swapchain: vk::SwapchainKHR,
) {
    let _ = unsafe { device.device_wait_idle() };
    if let Some(capture) = frame_capture {
        capture.destroy(device);
    }
    if let Some(timing) = gpu_timing {
        timing.destroy(device);
    }
    destroy_scene_present_frame_contexts(device, frame_contexts);
    unsafe {
        for semaphore in render_finished {
            device.destroy_semaphore(semaphore, None);
        }
        for view in swapchain_views {
            device.destroy_image_view(view, None);
        }
    }
    destroy_scene_gpu_resources(device, scene_resources);
    unsafe {
        device.destroy_command_pool(command_pool, None);
        device.destroy_swapchain_khr(swapchain, None);
    }
}
