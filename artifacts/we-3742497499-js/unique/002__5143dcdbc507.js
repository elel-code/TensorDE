'use strict';

export var scriptProperties = createScriptProperties()
	.addSlider({
		name: 'newSlider',
		label: 'New Slider',
		value: 50,
		min: 0,
		max: 100,
		integer: false
	})
	.finish();

/**
 * @param {Vec3} value - for property 'origin'
 * @return {Vec3} - update current property value
 */
export function update(value) {
	value.x = scriptProperties.newSlider
	return value;
}
