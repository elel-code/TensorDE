// source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
// json_path: ["objects", 58, "animationlayers", 1, "visible", "script"]
// metadata: {"id": 1146, "name": "胳膊外腿"}
// sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33

'use strict';

export var scriptProperties = createScriptProperties()
    .addSlider({
        name: 'percentage',
        label: 'Initial progress',
        value: 1,
        min: 0,
        max: 1,
        integer: false
    })
    .finish();

export function init(value) {
    const ani = 'addEndedCallback' in thisObject ? thisObject : thisObject.getAnimation()
    ani.play()
    ani.setFrame(ani.frameCount * scriptProperties.percentage)
    return value;
}
