// source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
// json_path: ["objects", 101, "angles", "script"]
// metadata: {"id": 602, "name": ""}
// sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb

'use strict';

export var scriptProperties = createScriptProperties()
	.addSlider({
		name: 'za',
		label: 'New Slider',
		value: 0,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'zb',
		label: 'New Slider',
		value: 0.5,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'zc',
		label: 'New Slider',
		value: 10,
		min: 0,
		max: 100,
		integer: false
	})
	.finish();

export function update(value) {
	value.z = scriptProperties.za + (Math.sin(engine.runtime * scriptProperties.zb) * scriptProperties.zc);
	return value;
}
