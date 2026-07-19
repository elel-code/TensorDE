//! Built-in SceneScript ES modules shared by convert and retained runtime contexts.

use rquickjs::Runtime;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};

const WE_MATH_MODULE: &str = r#"
export function clamp(value, minimum, maximum) {
    return Math.min(maximum, Math.max(minimum, value));
}
export function mix(left, right, amount) {
    return left + (right - left) * amount;
}
export function smoothstep(edge0, edge1, value) {
    const x = Math.min(1, Math.max(0, (value - edge0) / (edge1 - edge0)));
    return x * x * (3 - 2 * x);
}
export function deg2rad(value) { return value * Math.PI / 180; }
export function rad2deg(value) { return value * 180 / Math.PI; }
"#;

pub(crate) fn install(runtime: &Runtime) {
    runtime.set_loader(
        BuiltinResolver::default().with_module("WEMath"),
        BuiltinLoader::default().with_module("WEMath", WE_MATH_MODULE),
    );
}
