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
