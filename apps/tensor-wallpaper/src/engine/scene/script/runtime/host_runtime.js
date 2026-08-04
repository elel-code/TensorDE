(() => {
    const programs = [];
    const makeAudioBuffers = (resolution) => Object.freeze({
        left: new Float32Array(resolution),
        right: new Float32Array(resolution),
        average: new Float32Array(resolution),
    });
    const audio16 = makeAudioBuffers(16);
    const audio32 = makeAudioBuffers(32);
    const audio64 = makeAudioBuffers(64);
    const spectrumLeft = new Float32Array(64);
    const spectrumRight = new Float32Array(64);
    const pointer = new Vec2();
    const media = { state: 0, position: 0, duration: 0 };
    const texts = [];
    const consoleCapacity = 256;
    const consoleMessages = new Array(consoleCapacity);
    let consoleWriteIndex = 0;
    let consoleRetainedCount = 0;
    const recordConsole = (level, args) => {
        const message = args.map((value) => String(value)).join(' ');
        consoleMessages[consoleWriteIndex] = Object.freeze({ level, message });
        consoleWriteIndex = (consoleWriteIndex + 1) % consoleCapacity;
        consoleRetainedCount = Math.min(consoleCapacity, consoleRetainedCount + 1);
        globalThis.__tensor_wallpaperConsoleRetainedCount = consoleRetainedCount;
    };
    globalThis.__tensor_wallpaperConsoleMessages = consoleMessages;
    globalThis.__tensor_wallpaperConsoleRetainedCount = 0;
    globalThis.console = Object.freeze({
        log(...args) { recordConsole('log', args); },
        error(...args) { recordConsole('error', args); },
    });
    const emptyClicks = Object.freeze([]);
    globalThis.__tensor_wallpaperEmptyClicks = emptyClicks;
    let sceneEffects = [];
    let sceneEffectDirty = new Uint8Array(0);
    let sceneLayerByObject = new Map();
    let sceneLayerStateByObject = new Map();
    let sceneLayerStates = [];
    let sceneLayerDirty = new Uint8Array(0);
    let userProperties = Object.freeze(Object.create(null));
    let numeric = new Float64Array(0);
    const batch = { numeric, numericCount: 0, texts };
    globalThis.__tensor_wallpaperSpectrumLeft = spectrumLeft;
    globalThis.__tensor_wallpaperSpectrumRight = spectrumRight;
    globalThis.__tensor_wallpaperSetMedia = (state, position, duration) => {
        media.state = state;
        media.position = position;
        media.duration = duration;
    };
    globalThis.__tensor_wallpaperInstallHost = (host) => {
        const indexed = (selector, byIndex, byName, kind) => {
            if (typeof selector === 'number') {
                if (!Number.isSafeInteger(selector) || selector < 0) {
                    throw new TypeError(`SceneScript ${kind} numeric selector must be a non-negative safe integer`);
                }
                return Array.isArray(byIndex) ? byIndex[selector] : byIndex.get(selector);
            }
            if (typeof selector === 'string') return byName.get(selector);
            throw new TypeError(`SceneScript ${kind} selector must be a string or integer`);
        };
        const layerByName = new Map();
        const layerStateByProxy = new WeakMap();
        const layerOrder = [];
        sceneLayerByObject.clear();
        sceneLayerStateByObject.clear();
        sceneLayerStates = new Array(host.layers.length);
        sceneLayerDirty = new Uint8Array(host.layers.length);
        sceneEffects = new Array(host.effectCount);
        sceneEffectDirty = new Uint8Array(host.effectCount);
        const vector = (value, role) => {
            if (!value || typeof value !== 'object') {
                throw new TypeError(`SceneScript layer ${role} requires a Vec3`);
            }
            return new Vec3(value);
        };
        const scalar = (value, role) => {
            if (typeof value !== 'number' || !Number.isFinite(value)) {
                throw new TypeError(`SceneScript layer ${role} requires a finite number`);
            }
            return value;
        };
        const mark = (state, target) => { sceneLayerDirty[state.index] |= 1 << target; };
        for (const definition of host.layers) {
            const effectByName = new Map();
            const effectByIndex = new Map();
            for (const effect of definition.effects) {
                const state = {
                    binding: effect.binding,
                    object: definition.object,
                    visible: effect.visible,
                };
                const proxy = Object.freeze({
                    get visible() { return state.visible; },
                    set visible(value) {
                        if (typeof value !== 'boolean') {
                            throw new TypeError('SceneScript effect visible requires a boolean');
                        }
                        state.visible = value;
                        sceneEffectDirty[state.binding] = 1;
                    },
                });
                sceneEffects[effect.binding] = state;
                if (!effectByName.has(effect.name)) effectByName.set(effect.name, proxy);
                effectByIndex.set(effect.index, proxy);
            }
            const state = {
                index: definition.index,
                object: definition.object,
                parent: definition.parent,
                name: definition.name,
                origin: new Vec3(...definition.origin),
                angles: new Vec3(...definition.angles),
                scale: new Vec3(...definition.scale),
                color: new Vec3(...definition.color),
                alpha: definition.alpha,
                visible: definition.visible,
                size: new Vec2(...definition.size),
                alignment: definition.alignment,
                font: definition.font,
                soundPlaying: false,
            };
            const layer = {
                get name() { return state.name; },
                get origin() { return state.origin; },
                set origin(value) { state.origin = vector(value, 'origin'); mark(state, 1); },
                get angles() { return state.angles; },
                set angles(value) { state.angles = vector(value, 'angles'); mark(state, 2); },
                get scale() { return state.scale; },
                set scale(value) { state.scale = vector(value, 'scale'); mark(state, 3); },
                get color() { return state.color; },
                set color(value) { state.color = vector(value, 'color'); mark(state, 4); },
                get alpha() { return state.alpha; },
                set alpha(value) { state.alpha = scalar(value, 'alpha'); mark(state, 5); },
                get visible() { return state.visible; },
                set visible(value) {
                    if (typeof value !== 'boolean') {
                        throw new TypeError('SceneScript layer visible requires a boolean');
                    }
                    state.visible = value;
                    mark(state, 6);
                },
                get size() { return state.size; },
                get alignment() { return state.alignment; },
                set alignment(value) {
                    if (typeof value !== 'string' || value.length === 0) {
                        throw new TypeError('SceneScript layer alignment requires a non-empty string');
                    }
                    state.alignment = value;
                },
                get font() { return state.font; },
                set font(value) {
                    if (typeof value !== 'string') {
                        throw new TypeError(`SceneScript layer ${definition.name} font requires an asset path string`);
                    }
                    if (!definition.text) {
                        state.font = value;
                        return;
                    }
                    if (definition.font === null) {
                        throw new TypeError(`SceneScript text layer ${definition.name} has no baked font resource`);
                    }
                    if (value !== definition.font) {
                        throw new RangeError(`SceneScript layer ${definition.name} font ${value} is not baked into this v21 artifact`);
                    }
                    state.font = value;
                },
                getEffect(selector) {
                    const effect = indexed(selector, effectByIndex, effectByName, 'effect');
                    if (effect === undefined) {
                        throw new RangeError(`SceneScript effect not found on layer ${definition.name}: ${selector}`);
                    }
                    return effect;
                },
                getParent() {
                    if (state.parent === null) return undefined;
                    return sceneLayerByObject.get(state.parent);
                },
            };
            if (definition.sound) {
                layer.play = () => { state.soundPlaying = true; };
                layer.pause = () => { state.soundPlaying = false; };
                layer.stop = () => { state.soundPlaying = false; };
                layer.isPlaying = () => state.soundPlaying;
            }
            if (!layerByName.has(definition.name)) layerByName.set(definition.name, layer);
            layerOrder[definition.index] = layer;
            layerStateByProxy.set(layer, state);
            sceneLayerStates[definition.index] = state;
            sceneLayerByObject.set(definition.object, layer);
            sceneLayerStateByObject.set(definition.object, state);
        }
        userProperties = Object.freeze(host.userProperties);
        globalThis.thisScene = Object.freeze({
            getLayer(selector) {
                return indexed(selector, layerOrder, layerByName, 'layer');
            },
            getLayerCount() { return layerOrder.length; },
            enumerateLayers() { return layerOrder.slice(); },
            getLayerIndex(selector) {
                const layer = typeof selector === 'string'
                    ? layerByName.get(selector)
                    : selector;
                const state = layer && typeof layer === 'object'
                    ? layerStateByProxy.get(layer)
                    : undefined;
                return state === undefined ? -1 : layerOrder.indexOf(layer);
            },
            sortLayer() {
                throw new Error('SceneScript dynamic layer sorting is not represented by the retained render graph');
            },
        });
        engine.canvasSize = new Vec2(...host.canvasSize);
        engine.screenResolution = new Vec2(...host.canvasSize);
    };
    globalThis.__tensor_wallpaperSetCurrentLayer = (object) => {
        const layer = sceneLayerByObject.get(object);
        if (layer === undefined) {
            throw new RangeError(`SceneScript object has no layer: ${object}`);
        }
        globalThis.thisLayer = layer;
    };

    globalThis.engine = {
        runtime: 0,
        frametime: 0,
        AUDIO_RESOLUTION_16: 16,
        AUDIO_RESOLUTION_32: 32,
        AUDIO_RESOLUTION_64: 64,
        registerAudioBuffers(resolution = 16) {
            if (resolution === 16) return audio16;
            if (resolution === 32) return audio32;
            if (resolution === 64) return audio64;
            throw new RangeError('Resolution must be either 16, 32 or 64.');
        },
        registerAsset(path) { return path; },
    };
    globalThis.MediaPlaybackEvent = Object.freeze({
        PLAYBACK_STOPPED: 0,
        PLAYBACK_PLAYING: 1,
        PLAYBACK_PAUSED: 2,
    });
    globalThis.WEMath = Object.freeze({
        clamp(value, minimum, maximum) {
            return Math.min(maximum, Math.max(minimum, value));
        },
        mix(left, right, amount) { return left + (right - left) * amount; },
        smoothStep(edge0, edge1, value) {
            const x = Math.min(1, Math.max(0, (value - edge0) / (edge1 - edge0)));
            return x * x * (3 - 2 * x);
        },
        deg2rad(value) { return value * Math.PI / 180; },
        rad2deg(value) { return value * 180 / Math.PI; },
    });
    globalThis.createScriptProperties = () => {
        const values = Object.create(null);
        const builder = {
            addSlider(definition) { values[definition.name] = definition.value; return builder; },
            addCheckbox(definition) { values[definition.name] = definition.value; return builder; },
            addCombo(definition) { values[definition.name] = definition.value; return builder; },
            addColor(definition) { values[definition.name] = definition.value; return builder; },
            addText(definition) { values[definition.name] = definition.value; return builder; },
            finish() { return values; },
        };
        return builder;
    };
    globalThis.thisObject = Object.freeze({
        getAnimation() { return this; },
        play() {},
        setFrame() {},
        addEndedCallback() {},
        frameCount: 1,
    });
    globalThis.input = {
        cursorPosition: pointer,
        cursorWorldPosition: pointer,
    };
    const screenStorage = new Map();
    const globalStorage = new Map();
    const storageFor = (location = 'screen') => {
        if (location === 'screen') return screenStorage;
        if (location === 'global') return globalStorage;
        throw new RangeError(`SceneScript localStorage location must be screen or global: ${location}`);
    };
    const storageKey = (key) => {
        if (typeof key !== 'string') {
            throw new TypeError('SceneScript localStorage key must be a string');
        }
        return key;
    };
    globalThis.localStorage = Object.freeze({
        LOCATION_GLOBAL: 'global',
        LOCATION_SCREEN: 'screen',
        set(key, value, location = 'screen') {
            storageFor(location).set(storageKey(key), value);
        },
        get(key, location = 'screen') {
            return storageFor(location).get(storageKey(key));
        },
        delete(key, location = 'screen') {
            return storageFor(location).delete(storageKey(key));
        },
        clear(location = 'screen') { storageFor(location).clear(); },
    });
    globalThis.shared = Object.create(null);
    globalThis.thisLayer = { font: null };

    function initialValue(metadata) {
        if (metadata.target <= 4) {
            return new Vec3(metadata.initial[0], metadata.initial[1], metadata.initial[2]);
        }
        if (metadata.target === 5) return metadata.initial[0];
        if (metadata.target === 6) return metadata.initial[0];
        if (metadata.target === 7) return metadata.initialText;
        return metadata.initial[0];
    }

    function setBoundState(state, target, value) {
        if (target >= 1 && target <= 4) {
            if (typeof value !== 'number' && (!value || typeof value !== 'object')) {
                throw new TypeError(`SceneScript object ${state.object} target ${target} requires a Vec3`);
            }
            const vector = new Vec3(value);
            if (target === 1) state.origin = vector;
            else if (target === 2) state.angles = vector;
            else if (target === 3) state.scale = vector;
            else state.color = vector;
        } else if (target === 5) {
            if (typeof value !== 'number' || !Number.isFinite(value)) {
                throw new TypeError(`SceneScript object ${state.object} alpha target requires a finite number`);
            }
            state.alpha = value;
        } else if (target === 6) {
            state.visible = Boolean(value);
        }
    }

    globalThis.__tensor_wallpaperRegister = (namespace, metadata, properties) => {
        const layer = sceneLayerByObject.get(metadata.object);
        const layerState = sceneLayerStateByObject.get(metadata.object);
        if (layer === undefined) {
            throw new RangeError(`SceneScript object has no layer: ${metadata.object}`);
        }
        globalThis.thisLayer = layer;
        if (namespace.scriptProperties && properties) {
            for (const [name, bound] of Object.entries(properties)) {
                let value = bound;
                if (bound && typeof bound === 'object') {
                    if ('user' in bound) {
                        if (typeof bound.user !== 'string') {
                            throw new TypeError(`SceneScript property ${name} user binding must be a string`);
                        }
                        if (!Object.hasOwn(userProperties, bound.user)) {
                            throw new RangeError(`SceneScript property ${name} references unknown user property ${bound.user}`);
                        }
                        value = userProperties[bound.user];
                    } else if ('value' in bound) {
                        value = bound.value;
                    }
                }
                namespace.scriptProperties[name] = value;
            }
        }
        let value = initialValue(metadata);
        if (typeof namespace.init === 'function') {
            const initialized = namespace.init(value);
            if (initialized !== undefined) value = initialized;
        }
        setBoundState(layerState, metadata.target, value);
        if (typeof namespace.applyUserProperties === 'function') {
            namespace.applyUserProperties(userProperties);
        }
        programs.push({
            update: namespace.update,
            mediaPlaybackChanged: namespace.mediaPlaybackChanged,
            mediaTimelineChanged: namespace.mediaTimelineChanged,
            mediaPropertiesChanged: namespace.mediaPropertiesChanged,
            cursorClick: namespace.cursorClick,
            layer,
            layerState,
            object: metadata.object,
            target: metadata.target,
            selector: metadata.selector,
            subscriptions: metadata.subscriptions,
            value,
            published: false,
        });
    };

    function ensureNumericCapacity(entryCount) {
        const requiredLanes = entryCount * 7;
        if (numeric.length >= requiredLanes) return;
        const replacement = new Float64Array(requiredLanes);
        replacement.set(numeric);
        numeric = replacement;
        batch.numeric = numeric;
    }

    globalThis.__tensor_wallpaperDispatch = (time, frameTime, eventMask, pointerX, pointerY, clicks) => {
        engine.runtime = time;
        engine.frametime = frameTime;
        pointer.x = pointerX;
        pointer.y = pointerY;
        engine.pointer = pointer;
        if ((eventMask & 4) !== 0) {
            for (let i = 0; i < 64; i++) {
                const left = spectrumLeft[i] || 0;
                const right = spectrumRight[i] || 0;
                audio64.left[i] = left;
                audio64.right[i] = right;
                audio64.average[i] = 0.5 * (left + right);
            }
            for (let i = 0; i < 32; i++) {
                const source = 2 * i;
                audio32.left[i] = Math.max(audio64.left[source], audio64.left[source + 1]);
                audio32.right[i] = Math.max(audio64.right[source], audio64.right[source + 1]);
                audio32.average[i] = Math.max(audio64.average[source], audio64.average[source + 1]);
            }
            for (let i = 0; i < 16; i++) {
                const source = 2 * i;
                audio16.left[i] = Math.max(audio32.left[source], audio32.left[source + 1]);
                audio16.right[i] = Math.max(audio32.right[source], audio32.right[source + 1]);
                audio16.average[i] = Math.max(audio32.average[source], audio32.average[source + 1]);
            }
        }
        ensureNumericCapacity(programs.length + sceneEffects.length);
        texts.length = 0;
        let numericCount = 0;
        for (const program of programs) {
            const initialize = !program.published;
            if (!initialize && (program.subscriptions & eventMask) === 0) continue;
            globalThis.thisLayer = program.layer;
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaPlaybackChanged === 'function') {
                program.mediaPlaybackChanged(media);
            }
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaTimelineChanged === 'function') {
                program.mediaTimelineChanged(media);
            }
            if ((eventMask & 32) !== 0 &&
                typeof program.mediaPropertiesChanged === 'function') {
                program.mediaPropertiesChanged(media);
            }
            if (typeof program.cursorClick === 'function') {
                for (const click of clicks) {
                    if (click.object === program.object) program.cursorClick(click);
                }
            }
            let output = program.value;
            if (typeof program.update === 'function' &&
                (program.subscriptions & eventMask) !== 0) {
                const resolved = program.update(program.value);
                output = resolved === undefined ? program.value : resolved;
            }
            program.value = output;
            setBoundState(program.layerState, program.target, output);
            program.published = true;
            if (program.target === 7) {
                texts.push([program.object, String(output)]);
                continue;
            }
            const base = numericCount * 7;
            numeric[base] = program.object;
            numeric[base + 1] = program.target;
            numeric[base + 2] = program.selector;
            if (program.target <= 4) {
                if (typeof output === 'number') {
                    const scalar = Number(output);
                    numeric[base + 3] = scalar;
                    numeric[base + 4] = scalar;
                    numeric[base + 5] = scalar;
                } else {
                    numeric[base + 3] = Number(output.x);
                    numeric[base + 4] = Number(output.y);
                    numeric[base + 5] = Number(output.z);
                }
            } else {
                numeric[base + 3] = program.target === 6 ? (output ? 1 : 0) : Number(output);
            }
            numericCount++;
        }
        for (const state of sceneLayerStates) {
            const dirty = sceneLayerDirty[state.index];
            if (dirty === 0) continue;
            for (let target = 1; target <= 6; target++) {
                if ((dirty & (1 << target)) === 0) continue;
                ensureNumericCapacity(numericCount + 1);
                const base = numericCount * 7;
                numeric[base] = state.object;
                numeric[base + 1] = target;
                numeric[base + 2] = 0;
                const value = target === 1 ? state.origin
                    : target === 2 ? state.angles
                    : target === 3 ? state.scale
                    : target === 4 ? state.color
                    : target === 5 ? state.alpha
                    : state.visible;
                if (target <= 4) {
                    numeric[base + 3] = value.x;
                    numeric[base + 4] = value.y;
                    numeric[base + 5] = value.z;
                } else {
                    numeric[base + 3] = target === 6 ? (value ? 1 : 0) : value;
                    numeric[base + 4] = 0;
                    numeric[base + 5] = 0;
                }
                numeric[base + 6] = 0;
                numericCount++;
            }
            sceneLayerDirty[state.index] = 0;
        }
        for (let binding = 0; binding < sceneEffects.length; binding++) {
            if (sceneEffectDirty[binding] === 0) continue;
            const effect = sceneEffects[binding];
            if (effect === undefined) {
                throw new TypeError(`SceneScript host has no effect binding ${binding}`);
            }
            ensureNumericCapacity(numericCount + 1);
            const base = numericCount * 7;
            numeric[base] = effect.object;
            numeric[base + 1] = 9;
            numeric[base + 2] = binding;
            numeric[base + 3] = effect.visible ? 1 : 0;
            numeric[base + 4] = 0;
            numeric[base + 5] = 0;
            numeric[base + 6] = 0;
            sceneEffectDirty[binding] = 0;
            numericCount++;
        }
        batch.numericCount = numericCount;
        return batch;
    };
})();
