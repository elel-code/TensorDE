// source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
// json_path: ["objects", 48, "origin", "script"]
// metadata: {"id": 785, "name": "角色后"}
// sha256: 5143dcdbc5072615b21ee00e2039eefa0e694efa09740cab039e75faf7826207

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
