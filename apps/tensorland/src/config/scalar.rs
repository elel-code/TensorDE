//! Scalar configuration values validated during streaming KDL decode.
//!
//! Keeping single-field policy here lets `tensor-kdl` anchor failures to the
//! property entry before the source is lowered into renderer-independent
//! configuration state.

use tensor_kdl::{CtxResult, DecodeScalar, ErrorCode, ErrorCtx, Value};
use tensor_util::OutputScale;

use crate::layout::LayoutLength;

use super::OutputMode;

#[derive(Debug)]
pub(super) struct LimitedLayoutGap(pub(super) u32);

impl<'a> DecodeScalar<'a> for LimitedLayoutGap {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = u32::decode_scalar(value)?;
        (value <= 100_000).then_some(Self(value)).ok_or_else(|| {
            ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("layout gaps must be at most 100000 logical pixels")
        })
    }
}

#[derive(Debug)]
pub(super) struct LimitedOverviewGap(pub(super) u32);

impl<'a> DecodeScalar<'a> for LimitedOverviewGap {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = u32::decode_scalar(value)?;
        (value <= 100_000).then_some(Self(value)).ok_or_else(|| {
            ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("overview gaps must be at most 100000 logical pixels")
        })
    }
}

#[derive(Debug)]
pub(super) struct ParsedLayoutProportion(pub(super) LayoutLength);

impl<'a> DecodeScalar<'a> for ParsedLayoutProportion {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = f64::decode_scalar(value)?;
        if !value.is_finite() || !(0.0001..=10_000.0).contains(&value) {
            return Err(ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("layout proportion must be between 0.0001 and 10000"));
        }
        let scaled = (value * 10_000.0).round() as u32;
        Ok(Self(LayoutLength::proportion(scaled, 10_000)))
    }
}

#[derive(Debug)]
pub(super) struct PositiveLayoutFixed(pub(super) LayoutLength);

impl<'a> DecodeScalar<'a> for PositiveLayoutFixed {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = u32::decode_scalar(value)?;
        (value > 0)
            .then_some(Self(LayoutLength::fixed(value)))
            .ok_or_else(|| {
                ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                    .with_message("fixed layout width must be greater than zero")
            })
    }
}

#[derive(Debug)]
pub(super) struct ParsedOutputScale(pub(super) OutputScale);

impl<'a> DecodeScalar<'a> for ParsedOutputScale {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = f64::decode_scalar(value)?;
        OutputScale::from_f64(value).map(Self).ok_or_else(|| {
            ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("output scale must be finite and between 0.1 and 10")
        })
    }
}

#[derive(Debug)]
pub(super) struct ParsedOutputMode(pub(super) OutputMode);

impl<'a> DecodeScalar<'a> for ParsedOutputMode {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = String::decode_scalar(value)?;
        parse_output_mode(&value).map(Self).map_err(|message| {
            ErrorCtx::new(ErrorCode::InvalidNumber, 0)
                .with_message(format!("invalid output mode {value:?}: {message}"))
        })
    }
}

#[derive(Debug)]
pub(super) struct PositiveRefreshCap(pub(super) u32);

impl<'a> DecodeScalar<'a> for PositiveRefreshCap {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = u32::decode_scalar(value)?;
        (value > 0).then_some(Self(value)).ok_or_else(|| {
            ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("max-refresh-millihertz must be greater than zero")
        })
    }
}

fn parse_output_mode(value: &str) -> Result<OutputMode, String> {
    let (resolution, refresh) = match value.split_once('@') {
        Some((resolution, refresh)) if !refresh.contains('@') => (resolution, Some(refresh)),
        Some(_) => return Err("contains more than one `@` separator".to_owned()),
        None => (value, None),
    };
    let (width, height) = resolution
        .split_once('x')
        .filter(|(_, height)| !height.contains('x'))
        .ok_or_else(|| {
            "must use the form `<width>x<height>` or `<width>x<height>@<Hz>`".to_owned()
        })?;
    let width = parse_mode_dimension(width, "width")?;
    let height = parse_mode_dimension(height, "height")?;
    let refresh_millihertz = refresh.map(parse_refresh_millihertz).transpose()?;
    Ok(OutputMode::new(width, height, refresh_millihertz))
}

fn parse_mode_dimension(value: &str, name: &str) -> Result<u32, String> {
    let dimension = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a positive unsigned integer"))?;
    (dimension > 0)
        .then_some(dimension)
        .ok_or_else(|| format!("{name} must be greater than zero"))
}

fn parse_refresh_millihertz(value: &str) -> Result<u32, String> {
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) if !fraction.contains('.') => (whole, Some(fraction)),
        Some(_) => return Err("refresh rate has more than one decimal point".to_owned()),
        None => (value, None),
    };
    let whole = whole
        .parse::<u32>()
        .map_err(|_| "refresh rate must be a positive decimal number".to_owned())?;
    let fraction = match fraction {
        None => 0,
        Some("") => {
            return Err("refresh rate must contain digits after the decimal point".to_owned());
        }
        Some(value) if value.len() > 3 || !value.bytes().all(|byte| byte.is_ascii_digit()) => {
            return Err("refresh rate accepts at most three decimal places".to_owned());
        }
        Some(value) => value
            .parse::<u32>()
            .expect("a validated decimal fraction parses")
            .checked_mul(10_u32.pow(u32::try_from(3 - value.len()).unwrap_or(0)))
            .expect("a three-digit refresh fraction fits u32"),
    };
    let millihertz = whole
        .checked_mul(1_000)
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or_else(|| "refresh rate is too large".to_owned())?;
    if millihertz == 0 {
        return Err("refresh rate must be greater than zero".to_owned());
    }
    if millihertz > i32::MAX as u32 {
        return Err("refresh rate exceeds the DRM millihertz range".to_owned());
    }
    Ok(millihertz)
}
