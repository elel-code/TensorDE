/* ==== script 001 001__source-261__prop-ui_editor_properties_5_sector_1___unknown__ae51d52fe2a3.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 14, "effects", 0, "passes", 0, "constant_shader_values", "ui_editor_properties_5_sector_1_width", "script"]
metadata: {"id": 261}
sha256: ae51d52fe2a343dc77f6384bdb8fa4d3aa9a33a48e831d1b58e29f89f05068b1
*/
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


/* ==== script 002 002__source-1146__prop-visible__胳膊外腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 18, "children", 0, "provenance", "animation_layers", 1, "visible", "script"]
metadata: {"id": 1146, "name": "胳膊外腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 003 003__source-1148__prop-visible__内腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 18, "children", 0, "provenance", "animation_layers", 2, "visible", "script"]
metadata: {"id": 1148, "name": "内腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 004 004__source-1150__prop-visible__尾巴__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 18, "children", 0, "provenance", "animation_layers", 3, "visible", "script"]
metadata: {"id": 1150, "name": "尾巴"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 005 005__source-947__prop-visible__胳膊外腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 19, "children", 0, "provenance", "animation_layers", 1, "visible", "script"]
metadata: {"id": 947, "name": "胳膊外腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 006 006__source-951__prop-visible__内腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 19, "children", 0, "provenance", "animation_layers", 2, "visible", "script"]
metadata: {"id": 951, "name": "内腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 007 007__source-953__prop-visible__尾巴__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/assets/scene.gscene.json
json_path: ["nodes", 19, "children", 0, "provenance", "animation_layers", 3, "visible", "script"]
metadata: {"id": 953, "name": "尾巴"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 008 008__source-1090__prop-parallaxDepth__unknown__e7aae0317461.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 0, "parallaxDepth", "script"]
metadata: {"id": 1090, "name": ""}
sha256: e7aae0317461f4ede5b48beca842357ee9d85909d8578b761b637091ebdd69e8
*/
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

/* ==== script 009 009__source-1090__prop-visible__unknown__905088f5a51c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 0, "visible", "script"]
metadata: {"id": 1090, "name": ""}
sha256: 905088f5a51ceed3a92a4dedd4f2dbe90181eccf4c42947bbb593374141307e9
*/
'use strict';
export var scriptProperties = createScriptProperties()
    .addText({
        name: '麻匪',
        label: `<center><a href="https://space.bilibili.com/111174060/">  <img src="https://i.ibb.co/6hsBG5h/image.gif" width="100%" /> </a> </center>
 </a> <br><br><font color=#ffffff> <h5>壁纸由【Hope 麻匪】制作<br>Steam 版 Wallpaper Engine 专用<br>壁纸免费禁止售卖<br>针对个人用户：拆解壁纸 进行学习，参考，魔改 包括发布 【可随意使用 不需要经过我本人同意】<br>禁止中国大陆内任何壁纸软件使用<br>如需商业用途：可在Bilibili平台 私信本人进行授权<br>未经授权商用 禁止：软件，App, 小程序，网页，视频带货等笔记本作为显示宣传<br><br></h5><h6>This wallpaper is made by [HopeMafei].<br>It is exclusive to the Steam version of Wallpaper Engine.</br>The wallpaper is free and selling is prohibited.</br>For individual users:You can disassemble the wallpaper for learning, reference, and modification, including publishing.</br>[You can use it freely without my consent.]</br>It is prohibited to be used by any wallpaper software in mainland China.</br>For commercial use:You can send me a private message on the Bilibili platform for authorization.</br>Commercial use without authorization is prohibited:For software, apps, mini-programs, web pages, video live - streaming with goods, etc. using laptops as display for promotion.</h6><br><a href="https://steamcommunity.com/sharedfiles/filedetails/?id=2860307735"><img src="https://i.ibb.co/Cs0sGBYN/image.gif" width="100%"/> </a> </center>`,
        value: 'https://steamcommunity.com/profiles/76561198109148999/',
  	})
.finish();

/* ==== script 010 010__source-107__prop-angles__x1__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 10, "angles", "script"]
metadata: {"id": 107, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 011 011__source-107__prop-origin__x1__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 10, "origin", "script"]
metadata: {"id": 107, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 012 012__source-985__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 100, "text", "script"]
metadata: {"id": 985, "name": "Time and Date", "parent": 967}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 013 013__source-602__prop-angles__unknown__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 101, "angles", "script"]
metadata: {"id": 602, "name": ""}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 014 014__source-602__prop-origin__unknown__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 101, "origin", "script"]
metadata: {"id": 602, "name": ""}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 015 015__source-601__prop-angles__unknown__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 104, "angles", "script"]
metadata: {"id": 601, "name": ""}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 016 016__source-601__prop-origin__unknown__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 104, "origin", "script"]
metadata: {"id": 601, "name": ""}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 017 017__source-96__prop-angles__x1__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 11, "angles", "script"]
metadata: {"id": 96, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 018 018__source-96__prop-origin__x1__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 11, "origin", "script"]
metadata: {"id": 96, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 019 019__source-3465__prop-text__unknown__d604402eacf9.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 111, "text", "script"]
metadata: {"id": 3465, "name": "", "parent": 735}
sha256: d604402eacf939df5b7404b119118995ad43648c7314ca34d5debd4ec58f065f
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text3')){
      //修改text1
        switch(userProperties.text3){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 020 020__source-955__prop-text__unknown__d604402eacf9.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 116, "text", "script"]
metadata: {"id": 955, "name": "", "parent": 736}
sha256: d604402eacf939df5b7404b119118995ad43648c7314ca34d5debd4ec58f065f
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text3')){
      //修改text1
        switch(userProperties.text3){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 021 021__source-99__prop-angles__x1__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 12, "angles", "script"]
metadata: {"id": 99, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 022 022__source-99__prop-origin__x1__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 12, "origin", "script"]
metadata: {"id": 99, "image": "models/x1.json", "name": "x1", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 023 023__source-110__prop-angles__x2__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 13, "angles", "script"]
metadata: {"id": 110, "image": "models/x2.json", "name": "x2", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 024 024__source-110__prop-origin__x2__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 13, "origin", "script"]
metadata: {"id": 110, "image": "models/x2.json", "name": "x2", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 025 025__source-97__prop-angles__x2__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 14, "angles", "script"]
metadata: {"id": 97, "image": "models/x2.json", "name": "x2", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 026 026__source-97__prop-origin__x2__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 14, "origin", "script"]
metadata: {"id": 97, "image": "models/x2.json", "name": "x2", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 027 027__source-113__prop-angles__x3__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 15, "angles", "script"]
metadata: {"id": 113, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 028 028__source-113__prop-origin__x3__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 15, "origin", "script"]
metadata: {"id": 113, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 029 029__source-98__prop-angles__x3__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 16, "angles", "script"]
metadata: {"id": 98, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 030 030__source-98__prop-origin__x3__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 16, "origin", "script"]
metadata: {"id": 98, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 031 031__source-100__prop-angles__x3__a7105e20413c.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 17, "angles", "script"]
metadata: {"id": 100, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: a7105e20413ca74874a850a05ec86b5a57f38dd61262de70bf4bd77c1705f4cb
*/
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

/* ==== script 032 032__source-100__prop-origin__x3__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 17, "origin", "script"]
metadata: {"id": 100, "image": "models/x3.json", "name": "x3", "parent": 95}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 033 033__source-298__prop-origin__水珠2__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 19, "origin", "script"]
metadata: {"id": 298, "image": "models/水珠2.json", "name": "水珠2", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 034 034__source-321__prop-origin__水珠2__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 20, "origin", "script"]
metadata: {"id": 321, "image": "models/水珠2.json", "name": "水珠2", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 035 035__source-345__prop-origin__水珠3__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 21, "origin", "script"]
metadata: {"id": 345, "image": "models/水珠3.json", "name": "水珠3", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 036 036__source-309__prop-origin__水珠__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 22, "origin", "script"]
metadata: {"id": 309, "image": "models/水珠.json", "name": "水珠", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 037 037__source-365__prop-origin__水珠__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 23, "origin", "script"]
metadata: {"id": 365, "image": "models/水珠.json", "name": "水珠", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 038 038__source-368__prop-origin__水珠4__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 24, "origin", "script"]
metadata: {"id": 368, "image": "models/水珠4.json", "name": "水珠4", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 039 039__source-1310__prop-origin__水珠5__dfa2f1c8ddcc.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 25, "origin", "script"]
metadata: {"id": 1310, "image": "models/水珠5.json", "name": "水珠5", "parent": 667}
sha256: dfa2f1c8ddcc463ed97618dfa6b8d12e047c7924d6b2eba9906bc3c5a570a24f
*/
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

/* ==== script 040 040__source-652__prop-text__小文字__4a3a5615a692.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 27, "text", "script"]
metadata: {"id": 652, "name": "小文字", "parent": 603}
sha256: 4a3a5615a69279bd898782d66e9015b0c0d93bfbc204159e48bde98981894604
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text2')){
      //修改text1
        switch(userProperties.text2){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 041 041__source-2998__prop-text__默认主题大字__88b98570c441.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 28, "text", "script"]
metadata: {"id": 2998, "name": "默认主题大字", "parent": 603}
sha256: 88b98570c441992ba9a3f1054226a4f6fbf64bb273337f8f9fafb5484016f54c
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text1')){
      //修改text1
        switch(userProperties.text1){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 042 042__source-2578__prop-text__默认主题大字__88b98570c441.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 29, "text", "script"]
metadata: {"id": 2578, "name": "默认主题大字", "parent": 603}
sha256: 88b98570c441992ba9a3f1054226a4f6fbf64bb273337f8f9fafb5484016f54c
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text1')){
      //修改text1
        switch(userProperties.text1){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 043 043__source-914__prop-text__纯色主题大字__88b98570c441.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 30, "text", "script"]
metadata: {"id": 914, "name": "纯色主题大字", "parent": 603}
sha256: 88b98570c441992ba9a3f1054226a4f6fbf64bb273337f8f9fafb5484016f54c
*/
'use strict';
let font1   = engine.registerAsset("fonts/Jost-Medium.ttf");
let font2   = engine.registerAsset("fonts/Atami-Regular.otf");        
let font3   = engine.registerAsset("fonts/SmileySans-Oblique.ttf");		
let font4   = engine.registerAsset("fonts/方正琥珀简体.ttf");		
let font5   = engine.registerAsset("fonts/汉仪彩云体简.ttf");		
let font6   = engine.registerAsset("fonts/极限新圆.ttf");		
let font7   = engine.registerAsset("fonts/Aa后浪行楷.ttf");		
let font8   = engine.registerAsset("fonts/chaozishehaoxiashoushujianfan.ttf");		
let font9   = engine.registerAsset("fonts/李旭科书法.ttf");		
let font10  = engine.registerAsset("fonts/刘欢卡通手书1.07.ttf");		
let font11  = engine.registerAsset("fonts/千图笔锋手写体.ttf");		
let font12  = engine.registerAsset("fonts/禹卫书法行书简体.ttf");		
let font13  = engine.registerAsset("fonts/LEVIBRUSH.ttf");		
let font14  = engine.registerAsset("fonts/opentype (1).otf");		
let font15  = engine.registerAsset("fonts/Tourner (100).ttf");		
let font16  = engine.registerAsset("fonts/Tourner (101).ttf");		
let font17  = engine.registerAsset("fonts/Tourner (440).ttf");		
let font18  = engine.registerAsset("fonts/Tourner (544).ttf");	
let font19  = engine.registerAsset("fonts/Champignon.ttf");	
let font20  = engine.registerAsset("fonts/opentype (104).otf");	
let font21  = engine.registerAsset("fonts/SourceHanSans-Heavy.otf");
let font22  = engine.registerAsset("fonts/Imagination Station.ttf");
let font23  = engine.registerAsset("fonts/Aa攒劲小楷.ttf");
let font24  = engine.registerAsset("fonts/白路飞云手写体.ttf");
let font25  = engine.registerAsset("fonts/演示秋鸿楷.ttf");	
let font26  = engine.registerAsset("fonts/SF-Pro-Medium.otf");	
let font27  = engine.registerAsset("fonts/SF-Pro-Rounded-Heavy.otf");	
let font28  = engine.registerAsset("fonts/ELEPHNTI.ttf");	
let font29  = engine.registerAsset("fonts/方正行楷简体.ttf");
let font30  = engine.registerAsset("fonts/书法字体.ttf");
let font31  = engine.registerAsset("fonts/MapleMono-CN-Bold.ttf");
let font32  = engine.registerAsset("fonts/DouyinSansBold.ttf");
let font33  = engine.registerAsset("fonts/Tourner (537).ttf");
let font34  = engine.registerAsset("fonts/Tourner (339).ttf");



export function applyUserProperties(userProperties) {
    if(userProperties.hasOwnProperty('text1')){
      //修改text1
        switch(userProperties.text1){
            case("1"):
            thisLayer.font = font1;
            break;
            case("2"):
            thisLayer.font = font2;
            break;
            case("3"):
            thisLayer.font = font3;
            break;
            case("4"):
            thisLayer.font = font4;
            break;
            case("5"):
            thisLayer.font = font5;
            break;
            case("6"):
            thisLayer.font = font6;
            break;
            case("7"):
            thisLayer.font = font7;
            break;
            case("8"):
            thisLayer.font = font8;
            break;
            case("9"):
            thisLayer.font = font9;
            break;
            case("10"):
            thisLayer.font = font10;
            break;
            case("11"):
            thisLayer.font = font11;
            break;
            case("12"):
            thisLayer.font = font12;
            break;
            case("13"):
            thisLayer.font = font13;
            break;
            case("14"):
            thisLayer.font = font14;
            break;
            case("15"):
            thisLayer.font = font15;
            break;
            case("16"):
            thisLayer.font = font16;
            break;
            case("17"):
            thisLayer.font = font17;
            break;
            case("18"):
            thisLayer.font = font18;
            break;
            case("19"):
            thisLayer.font = font19;
            break;
            case("20"):
            thisLayer.font = font20;
            break;
            case("21"):
            thisLayer.font = font21;
            break;
            case("22"):
            thisLayer.font = font22;
            break;
            case("23"):
            thisLayer.font = font23;
            break;
            case("24"):
            thisLayer.font = font24;
            break;
            case("25"):
            thisLayer.font = font25;
			break;
            case("26"):
            thisLayer.font = font26;
			break;
            case("27"):
            thisLayer.font = font27;
            break;
            case("28"):
            thisLayer.font = font28;
            break;
            case("29"):
            thisLayer.font = font29;
            break;
            case("30"):
            thisLayer.font = font30;
            break;
            case("31"):
            thisLayer.font = font31;
            break;
            case("32"):
            thisLayer.font = font32;
            break;
            case("33"):
            thisLayer.font = font33;
            break;
            case("34"):
            thisLayer.font = font34;
            break;
        }
    }    
}

/* ==== script 044 044__source-252__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 33, "text", "script"]
metadata: {"id": 252, "name": "Time and Date", "parent": 1416}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 045 045__source-257__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 34, "text", "script"]
metadata: {"id": 257, "name": "Time and Date", "parent": 1416}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 046 046__source-997__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 37, "text", "script"]
metadata: {"id": 997, "name": "Time and Date", "parent": 991}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 047 047__source-1003__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 38, "text", "script"]
metadata: {"id": 1003, "name": "Time and Date", "parent": 991}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 048 048__source-261__prop-ui_editor_properties_5_sector_1___unknown__ae51d52fe2a3.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 39, "effects", 0, "passes", 0, "constantshadervalues", "ui_editor_properties_5_sector_1_width", "script"]
metadata: {"id": 261}
sha256: ae51d52fe2a343dc77f6384bdb8fa4d3aa9a33a48e831d1b58e29f89f05068b1
*/
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


/* ==== script 049 049__source-687__prop-origin__角色后-影子__5143dcdbc507.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 40, "origin", "script"]
metadata: {"id": 687, "name": "角色后 影子"}
sha256: 5143dcdbc5072615b21ee00e2039eefa0e694efa09740cab039e75faf7826207
*/
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


/* ==== script 050 050__source-785__prop-origin__角色后__5143dcdbc507.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 48, "origin", "script"]
metadata: {"id": 785, "name": "角色后"}
sha256: 5143dcdbc5072615b21ee00e2039eefa0e694efa09740cab039e75faf7826207
*/
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


/* ==== script 051 051__source-1141__prop-origin__角色主影子__5143dcdbc507.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 57, "origin", "script"]
metadata: {"id": 1141, "name": "角色主影子"}
sha256: 5143dcdbc5072615b21ee00e2039eefa0e694efa09740cab039e75faf7826207
*/
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


/* ==== script 052 052__source-1146__prop-visible__胳膊外腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 58, "animationlayers", 1, "visible", "script"]
metadata: {"id": 1146, "name": "胳膊外腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 053 053__source-1148__prop-visible__内腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 58, "animationlayers", 2, "visible", "script"]
metadata: {"id": 1148, "name": "内腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 054 054__source-1150__prop-visible__尾巴__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 58, "animationlayers", 3, "visible", "script"]
metadata: {"id": 1150, "name": "尾巴"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 055 055__source-579__prop-origin__角色主__5143dcdbc507.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 65, "origin", "script"]
metadata: {"id": 579, "name": "角色主"}
sha256: 5143dcdbc5072615b21ee00e2039eefa0e694efa09740cab039e75faf7826207
*/
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


/* ==== script 056 056__source-947__prop-visible__胳膊外腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 66, "animationlayers", 1, "visible", "script"]
metadata: {"id": 947, "name": "胳膊外腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 057 057__source-951__prop-visible__内腿__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 66, "animationlayers", 2, "visible", "script"]
metadata: {"id": 951, "name": "内腿"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 058 058__source-953__prop-visible__尾巴__a0d8243be2ac.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 66, "animationlayers", 3, "visible", "script"]
metadata: {"id": 953, "name": "尾巴"}
sha256: a0d8243be2ac4b63002528076322846d937740aaa79ce2e20ead498fe173ee33
*/
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

/* ==== script 059 059__source-249__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 91, "text", "script"]
metadata: {"id": 249, "name": "Time and Date", "parent": 1419}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 060 060__source-258__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 92, "text", "script"]
metadata: {"id": 258, "name": "Time and Date", "parent": 1419}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 061 061__source-250__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 93, "text", "script"]
metadata: {"id": 250, "name": "Time and Date", "parent": 1419}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 062 062__source-259__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 94, "text", "script"]
metadata: {"id": 259, "name": "Time and Date", "parent": 1419}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 063 063__source-973__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 97, "text", "script"]
metadata: {"id": 973, "name": "Time and Date", "parent": 967}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 064 064__source-981__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 98, "text", "script"]
metadata: {"id": 981, "name": "Time and Date", "parent": 967}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}


/* ==== script 065 065__source-982__prop-text__Time-and-Date__671726fc05e8.js ====
source_file: /tmp/gilder-we-3742497499-output-current/metadata/source-scene.json
json_path: ["objects", 99, "text", "script"]
metadata: {"id": 982, "name": "Time and Date", "parent": 967}
sha256: 671726fc05e875965f50a52a61038411c6f8864056a062f619c7c25b6ad9d675
*/
// Please note: Do not remove this line or asset references may break.
export let __workshopId = '2452831710';
"use strict";export var scriptProperties=createScriptProperties().addCombo({name:"language",label:"Language",options:[{label:"English",value:"en-US"},{label:"繁體中文",value:"zh-TW"},{label:"日本語",value:"ja-JP"}]}).addCombo({name:"week",label:"Weekday Type",options:[{label:"Narrow (e.g. T)",value:"narrow"},{label:"Short (e.g. Thu)",value:"short"},{label:"Long (e.g. Thursday)",value:"long"}]}).addCheckbox({name:"twelve",label:"Use 12-hour clock",value:!1}).addText({name:"format",label:"Format",value:"yyyy/MM/dd hh:mm:ss"}).addText({name:"formatInfo",label:"Format Placeholders",value:" - Year\nyy: 2-digit year\nyyyy: 4-digit year\n\n - Month\nM: numeric month (e.g. 3)\nMM: 2-digit month (e.g. 03)\n\n - Day\nd: numeric day (e.g. 5)\ndd: 2-digit day (e.g. 05)\n\n - Weekday\nW: Weekday placeholder\n\n - Hour\nh: numeric hour (e.g. 4)\nhh: 2-digit hour (e.g. 04)\n\n - Minute\nm: numeric minute (e.g. 7)\nmm: 2-digit minute (e.g. 07)\n\n - Second\ns: numeric second (e.g. 1)\nss: 2-digit second (e.g. 01)\n\n - Millisecond\nS: Millisecond placeholder (Max up to 3 digits)\n\n - AMPM\na: AMPM placeholder\n※ Note ※\nYou must enable the 'Use 12-hour clock' to use this placeholder"}).finish();const weekday={"en-US":{narrow:["S","M","T","W","T","F","S"],short:["Sun","Mon","Tue","Wed","Thu","Fri","Sat"],long:["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]},"zh-TW":{narrow:["日","一","二","三","四","五","六"],short:["週日","週一","週二","週三","週四","週五","週六"],long:["星期日","星期一","星期二","星期三","星期四","星期五","星期六"]},"ja-JP":{narrow:["日","月","火","水","木","金","土"],short:["日曜","月曜","火曜","水曜","木曜","金曜","土曜"],long:["日曜日","月曜日","火曜日","水曜日","木曜日","金曜日","土曜日"]}},ampmlang={"en-US":["AM","PM"],"zh-TW":["上午","下午"],"ja-JP":["午前","午後"]};export function update(e){const t=new Date,r={year:"",month:"",day:"",week:"",hour:"",minute:"",second:"",millisecond:"",ampm:""},o={lang:scriptProperties.language,weektype:scriptProperties.week,twelve:scriptProperties.twelve,year:scriptProperties.format.match(/(?<!\\)y+/)?scriptProperties.format.match(/(?<!\\)y+/)[0]:"",month:scriptProperties.format.match(/(?<!\\)M+/)?scriptProperties.format.match(/(?<!\\)M+/)[0]:"",day:scriptProperties.format.match(/(?<!\\)d+/)?scriptProperties.format.match(/(?<!\\)d+/)[0]:"",week:scriptProperties.format.match(/(?<!\\)W+/)?scriptProperties.format.match(/(?<!\\)W+/)[0]:"",hour:scriptProperties.format.match(/(?<!\\)h+/)?scriptProperties.format.match(/(?<!\\)h+/)[0]:"",minute:scriptProperties.format.match(/(?<!\\)m+/)?scriptProperties.format.match(/(?<!\\)m+/)[0]:"",second:scriptProperties.format.match(/(?<!\\)s+/)?scriptProperties.format.match(/(?<!\\)s+/)[0]:"",millisecond:scriptProperties.format.match(/(?<!\\)S+/)?scriptProperties.format.match(/(?<!\\)S+/)[0]:"",ampm:scriptProperties.format.match(/(?<!\\)a/)?scriptProperties.format.match(/(?<!\\)a/)[0]:""};2==o.year.length?r.year=t.getFullYear().toString().slice(2):4==o.year.length?r.year=t.getFullYear().toString():r.year=o.year,1==o.month.length?r.month=(t.getMonth()+1).toString():2==o.month.length?r.month=1==(t.getMonth()+1).toString().length?"0"+(t.getMonth()+1).toString():(t.getMonth()+1).toString():r.month=o.month,1==o.day.length?r.day=t.getDate():2==o.day.length?r.day=1==t.getDate().toString().length?"0"+t.getDate().toString():t.getDate().toString():r.day=o.day,1==o.week.length?r.week=weekday[o.lang][o.weektype][t.getDay()]:r.week=o.week,o.twelve?1==o.hour.length?r.hour=t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():2==o.hour.length?r.hour=1==(t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString()).length?"0"+t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():t.getHours()>12?(t.getHours()-12).toString():t.getHours().toString():r.hour=o.hour:1==o.hour.length?r.hour=t.getHours():2==o.hour.length?r.hour=1==t.getHours().toString().length?"0"+t.getHours().toString():t.getHours().toString():r.hour=o.hour,1==o.minute.length?r.minute=t.getMinutes():2==o.minute.length?r.minute=1==t.getMinutes().toString().length?"0"+t.getMinutes().toString():t.getMinutes().toString():r.minute=o.minute,1==o.second.length?r.second=t.getSeconds():2==o.second.length?r.second=1==t.getSeconds().toString().length?"0"+t.getSeconds().toString():t.getSeconds().toString():r.second=o.second,1==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,1):2==o.millisecond.length?r.millisecond=t.getMilliseconds().toString().slice(0,2):3==o.millisecond.length?r.millisecond=t.getMilliseconds().toString():r.millisecond=o.millisecond,1==o.ampm.length&&o.twelve?r.ampm=t.getHours()>12?ampmlang[o.lang][1]:ampmlang[o.lang][0]:r.ampm="";let n=scriptProperties.format;return n=(n=(n=(n=(n=(n=(n=(n=(n=(n=n.replace(/(?<!\\)y+/,r.year)).replace(/(?<!\\)M+/,r.month)).replace(/(?<!\\)d+/,r.day)).replace(/(?<!\\)h+/,r.hour)).replace(/(?<!\\)m+/,r.minute)).replace(/(?<!\\)s+/,r.second)).replace(/(?<!\\)S+/,r.millisecond)).replace(/(?<!\\)a/,r.ampm)).replace(/(?<!\\)W+/,r.week)).replace(/\\/,"")}

