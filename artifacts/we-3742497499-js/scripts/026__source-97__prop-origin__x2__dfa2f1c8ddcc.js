// source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
// json_path: ["objects", 14, "origin", "script"]
// metadata: {"id": 97, "image": "models/x2.json", "name": "x2", "parent": 95}
// sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f

'use strict';

export var scriptProperties = createScriptProperties()
	.addSlider({
		name: 'xa',
		label: 'New Slider',
		value: 50,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'xb',
		label: 'New Slider',
		value: 0.3,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'xc',
		label: 'New Slider',
		value: 50,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'ya',
		label: 'New Slider',
		value: 50,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'yb',
		label: 'New Slider',
		value: 0.4,
		min: 0,
		max: 100,
		integer: false
	})
	.addSlider({
		name: 'yc',
		label: 'New Slider',
		value: 50,
		min: 0,
		max: 100,
		integer: false
	})
	.finish();

export function update(value) {
	value.x = scriptProperties.xa + (Math.sin(engine.runtime * scriptProperties.xb) * scriptProperties.xc);
	value.y = scriptProperties.ya + (Math.sin(engine.runtime * scriptProperties.yb) * scriptProperties.yc);
	return value;
}
