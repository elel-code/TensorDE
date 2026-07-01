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
