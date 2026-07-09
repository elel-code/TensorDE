//! Codec reference planning and streaming bootstrap helpers.
//!
//! This isolates the DPB/reference-plan state from the renderer loop, matching
//! the FFmpeg-style split between parsed access units, decoder state, and frame
//! presentation.

#![allow(dead_code)]

include!("codec_reference/h264_av1_reference.rs");
include!("codec_reference/h264_h265_planner.rs");
include!("codec_reference/reference_tests.rs");
