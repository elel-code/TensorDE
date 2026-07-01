// source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
// json_path: ["objects", 0, "parallaxDepth", "script"]
// metadata: {"id": 1090, "name": ""}
// sha256: e7aae0317461f4ede5b48beca842357ee9d85909d8578b761b637091ebdd69e8

'use strict';

export var scriptProperties = createScriptProperties()
	.addText({
		name: 'name',
		label: 'Layer names (逗号分隔)',
		value: '提示,弹窗' 
	})
	.addText({
		name: 'property',
		label: 'property name',
		value: 'newproperty'
	})
	.addSlider({
		name: 'seconds',
		label: 'seconds',
		value: 5,
		min: 0,
		max: 100,
		integer: false
	})
	.finish();

shared = {
    ts: true
};
var timer = 0;
var alphaValue = 1;

function getLayerNames() {
    return scriptProperties.name.split(',')
        .map(name => name.trim())
        .filter(name => name);
}

export function update(value) {
    timer += engine.frametime;
    const layerNames = getLayerNames(); 

    if (shared.ts) {
        for (const name of layerNames) {
            var layer = thisScene.getLayer(name);
            if (layer) {
                layer.visible = true;
                layer.alpha = alphaValue;

                if (timer >= scriptProperties.seconds) {
                    alphaValue -= engine.frametime;
                    if (alphaValue <= 0) {
                        alphaValue = 0;
                        layer.visible = false;
                        thisScene.destroyLayer(name);
                    }
                }
            }
        }
    }
    return value;
}

export function applyUserProperties(changedUserProperties) {
    const propKey = scriptProperties.property;
    if (changedUserProperties.hasOwnProperty(propKey)) {
        shared.ts = changedUserProperties[propKey];
        const layerNames = getLayerNames();

        if (!shared.ts) {
            for (const name of layerNames) {
                const layer = thisScene.getLayer(name);
                if (layer) {
                    thisScene.destroyLayer(layer);
                }
            }
        }
    }
}
