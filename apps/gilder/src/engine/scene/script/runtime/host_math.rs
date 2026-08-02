//! Retained SceneScript math globals installed before authored modules.
//!
//! Reference: `reverse-engineered/gilder/docs/javascript-api.md` math types.

pub(super) const HOST_MATH_PRELUDE: &str = r#"
(() => {
    const finite = (value, role) => {
        const number = Number(value);
        if (!Number.isFinite(number)) throw new TypeError(`Vec3 ${role} must be finite`);
        return number;
    };
    const components = (values) => {
        if (values.length === 0) return [0, 0, 0];
        if (values.length === 1) {
            const value = values[0];
            if (typeof value === 'number') return [value, value, value];
            if (typeof value === 'string') {
                const parsed = value.trim().split(/\s+/).map(Number);
                if (parsed.length < 1 || parsed.length > 3) {
                    throw new TypeError('Vec3 string requires one to three components');
                }
                return [parsed[0], parsed[1] ?? 0, parsed[2] ?? 0];
            }
            if (value && typeof value === 'object') {
                return [value.x ?? 0, value.y ?? 0, value.z ?? 0];
            }
        }
        return [values[0] ?? 0, values[1] ?? 0, values[2] ?? 0];
    };
    const operand = (value) => typeof value === 'number'
        ? [value, value, value]
        : components([value]);
    const smooth = (minimum, maximum, value) => {
        if (minimum === maximum) return value < minimum ? 0 : 1;
        const x = Math.min(1, Math.max(0, (value - minimum) / (maximum - minimum)));
        return x * x * (3 - 2 * x);
    };

    globalThis.Vec2 = class Vec2 {
        constructor(x = 0, y = x) {
            if (x && typeof x === 'object') {
                y = x.y ?? 0;
                x = x.x ?? 0;
            }
            this.x = finite(x, 'x');
            this.y = finite(y, 'y');
        }
        copy() { return new Vec2(this); }
        add(value) { const target = new Vec2(value); return new Vec2(this.x + target.x, this.y + target.y); }
        subtract(value) { const target = new Vec2(value); return new Vec2(this.x - target.x, this.y - target.y); }
        multiply(value) {
            const target = typeof value === 'number' ? new Vec2(value) : new Vec2(value);
            return new Vec2(this.x * target.x, this.y * target.y);
        }
        divide(value) {
            const target = typeof value === 'number' ? new Vec2(value) : new Vec2(value);
            return new Vec2(this.x / target.x, this.y / target.y);
        }
        lengthSqr() { return this.x * this.x + this.y * this.y; }
        length() { return Math.sqrt(this.lengthSqr()); }
        normalize() { const length = this.length(); return length === 0 ? new Vec2() : this.divide(length); }
        toString() { return `Vec2(${this.x}, ${this.y})`; }
        toConfigString() { return `${this.x} ${this.y}`; }
    };

    globalThis.Vec3 = class Vec3 {
        constructor(...values) {
            const [x, y, z] = components(values);
            this.x = finite(x, 'x');
            this.y = finite(y, 'y');
            this.z = finite(z, 'z');
        }
        lengthSqr() { return this.dot(this); }
        length() { return Math.sqrt(this.lengthSqr()); }
        distanceSqr(value) { return this.subtract(value).lengthSqr(); }
        distance(value) { return Math.sqrt(this.distanceSqr(value)); }
        normalize() { const length = this.length(); return length === 0 ? new Vec3() : this.divide(length); }
        copy() { return new Vec3(this); }
        equals(value) { const [x, y, z] = operand(value); return this.x === x && this.y === y && this.z === z; }
        isFinite() { return Number.isFinite(this.x) && Number.isFinite(this.y) && Number.isFinite(this.z); }
        negate() { return new Vec3(-this.x, -this.y, -this.z); }
        add(value) { const [x, y, z] = operand(value); return new Vec3(this.x + x, this.y + y, this.z + z); }
        subtract(value) { const [x, y, z] = operand(value); return new Vec3(this.x - x, this.y - y, this.z - z); }
        multiply(value) { const [x, y, z] = operand(value); return new Vec3(this.x * x, this.y * y, this.z * z); }
        divide(value) { const [x, y, z] = operand(value); return new Vec3(this.x / x, this.y / y, this.z / z); }
        dot(value) { const [x, y, z] = operand(value); return this.x * x + this.y * y + this.z * z; }
        cross(value) { const [x, y, z] = operand(value); return new Vec3(this.y * z - this.z * y, this.z * x - this.x * z, this.x * y - this.y * x); }
        reflect(normal) { const unit = new Vec3(normal); return this.subtract(unit.multiply(2 * this.dot(unit))); }
        project(value) { const target = new Vec3(value); const length = target.lengthSqr(); return length === 0 ? new Vec3() : target.multiply(this.dot(target) / length); }
        mix(value, amount) { return this.add(new Vec3(value).subtract(this).multiply(amount)); }
        min(value) { const [x, y, z] = operand(value); return new Vec3(Math.min(this.x, x), Math.min(this.y, y), Math.min(this.z, z)); }
        max(value) { const [x, y, z] = operand(value); return new Vec3(Math.max(this.x, x), Math.max(this.y, y), Math.max(this.z, z)); }
        clamp(minimum, maximum) { return this.max(minimum).min(maximum); }
        abs() { return new Vec3(Math.abs(this.x), Math.abs(this.y), Math.abs(this.z)); }
        sign() { return new Vec3(Math.sign(this.x), Math.sign(this.y), Math.sign(this.z)); }
        round() { return new Vec3(Math.round(this.x), Math.round(this.y), Math.round(this.z)); }
        floor() { return new Vec3(Math.floor(this.x), Math.floor(this.y), Math.floor(this.z)); }
        ceil() { return new Vec3(Math.ceil(this.x), Math.ceil(this.y), Math.ceil(this.z)); }
        fract() { return this.subtract(this.floor()); }
        mod(value) { const [x, y, z] = operand(value); return new Vec3(this.x % x, this.y % y, this.z % z); }
        step(edge) { const [x, y, z] = operand(edge); return new Vec3(this.x < x ? 0 : 1, this.y < y ? 0 : 1, this.z < z ? 0 : 1); }
        smoothStep(minimum, maximum) {
            const low = operand(minimum); const high = operand(maximum);
            return new Vec3(smooth(low[0], high[0], this.x), smooth(low[1], high[1], this.y), smooth(low[2], high[2], this.z));
        }
        angleBetween(value) {
            const target = new Vec3(value);
            const divisor = this.length() * target.length();
            return divisor === 0 ? 0 : Math.acos(Math.min(1, Math.max(-1, this.dot(target) / divisor))) * 180 / Math.PI;
        }
        toString() { return `Vec3(${this.x}, ${this.y}, ${this.z})`; }
        toConfigString() { return `${this.x} ${this.y} ${this.z}`; }
    };
})();
"#;
