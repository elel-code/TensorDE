'use strict';

/*
 * Adding new properties to the editor so you can tweak these values in the editor
 */
export var scriptProperties = createScriptProperties()
	.addSlider({
		name: 'frequency',
		label: 'ui_editor_properties_audio_frequency',
		value: 0,
		min: 0,
		max: 15,
		integer: true
	})
	.addSlider({
		name: 'smoothing',
		label: 'ui_editor_properties_audio_response',
		value: 15,
		min: 0,
		max: 25,
		integer: false
	})
	.addSlider({
		name: 'minvalue',
		label: 'ui_editor_properties_min',
		value: 0.8,
		min: 0,
		max: 3,
		integer: false
	})
	.addSlider({
		name: 'maxvalue',
		label: 'ui_editor_properties_max',
		value: 1.2,
		min: 0,
		max: 3,
		integer: false
	})
	.finish();

/**
 * This creates a permanent link to the audio response data.
 */
const audioBuffer = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_16);
let smoothValue = 0;
let initialValue;

/**
 * Calculate new audio-scaled value
 */
export function update() {
	const valueDelta = scriptProperties.maxvalue - scriptProperties.minvalue;
	const audioDelta = audioBuffer.average[scriptProperties.frequency] - smoothValue;
	
	smoothValue += audioDelta * Math.min(1.0, engine.frametime * scriptProperties.smoothing);
	smoothValue = Math.min(1.0, smoothValue);

	return initialValue * (smoothValue * valueDelta + scriptProperties.minvalue);
}

export function init(value) {
	initialValue = (typeof value === 'number') ? value : value.x;
}
