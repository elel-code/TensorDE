//! Bounded cold-upload recording for retained scene resources.

use vulkan_renderer::{Queue, Result, UploadBatch};

/// Records one cold upload through a bounded timeline-streamed staging belt.
///
/// A full batch is submitted without blocking first. Only if its immediate
/// retry still cannot reserve capacity do we wait for the oldest staging
/// submission, reclaim its chunks, and retry. Every submission remains on the
/// same graphics queue, preserving copy and transition order.
pub(in super::super) fn record_cold_upload<T>(
    uploads: &mut UploadBatch<'_>,
    queue: &Queue,
    mut record: impl FnMut(&mut UploadBatch<'_>) -> Result<T>,
) -> Result<T> {
    stream_after_upload_belt_exhaustion(
        uploads,
        |uploads| record(uploads),
        |uploads| uploads.has_staged_uploads(),
        |uploads| uploads.flush_for_reuse(queue, &[]).map(|_| ()),
        |uploads| uploads.wait_for_oldest_reuse(queue),
    )
}

fn stream_after_upload_belt_exhaustion<T, Context>(
    context: &mut Context,
    mut record: impl FnMut(&mut Context) -> Result<T>,
    mut has_staged_uploads: impl FnMut(&Context) -> bool,
    mut flush: impl FnMut(&mut Context) -> Result<()>,
    mut wait_for_reuse: impl FnMut(&mut Context) -> Result<()>,
) -> Result<T> {
    match record(context) {
        Ok(value) => Ok(value),
        Err(error) if error.is_upload_belt_exhausted() => {
            if !has_staged_uploads(context) {
                return Err(error);
            }
            flush(context)?;
            match record(context) {
                Ok(value) => Ok(value),
                Err(error) if error.is_upload_belt_exhausted() => {
                    wait_for_reuse(context)?;
                    record(context)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkan_renderer::{Error, UploadBeltLimit};

    #[test]
    fn streams_before_waiting_when_the_first_post_flush_retry_fits() {
        let mut calls = 0;
        let mut flushes = 0;
        let mut waits = 0;

        let value = stream_after_upload_belt_exhaustion(
            &mut (),
            |_| {
                calls += 1;
                if calls == 1 {
                    Err(Error::UploadBeltExhausted {
                        limit: UploadBeltLimit::ChunkCount(8),
                    })
                } else {
                    Ok(42)
                }
            },
            |_| true,
            |_| {
                flushes += 1;
                Ok(())
            },
            |_| {
                waits += 1;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(value, 42);
        assert_eq!(calls, 2);
        assert_eq!(flushes, 1);
        assert_eq!(waits, 0);
    }

    #[test]
    fn waits_only_after_the_post_flush_retry_still_exhausts_the_belt() {
        let mut calls = 0;
        let mut flushes = 0;
        let mut waits = 0;

        let value = stream_after_upload_belt_exhaustion(
            &mut (),
            |_| {
                calls += 1;
                if calls < 3 {
                    Err(Error::UploadBeltExhausted {
                        limit: UploadBeltLimit::ChunkCount(8),
                    })
                } else {
                    Ok(42)
                }
            },
            |_| true,
            |_| {
                flushes += 1;
                Ok(())
            },
            |_| {
                waits += 1;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(value, 42);
        assert_eq!(calls, 3);
        assert_eq!(flushes, 1);
        assert_eq!(waits, 1);
    }

    #[test]
    fn does_not_mask_an_unstreamable_first_upload() {
        let error = stream_after_upload_belt_exhaustion(
            &mut (),
            |_| {
                Err::<(), _>(Error::UploadBeltExhausted {
                    limit: UploadBeltLimit::RetainedBytes(32 * 1024 * 1024),
                })
            },
            |_| false,
            |_| unreachable!("an empty batch cannot make an upload fit"),
            |_| unreachable!("an empty batch cannot make an upload fit"),
        )
        .unwrap_err();

        assert!(error.is_upload_belt_exhausted());
    }
}
