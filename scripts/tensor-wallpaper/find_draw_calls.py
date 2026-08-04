"""Find and classify Draw/DrawIndexed-shaped call sites in WE exes."""
from __future__ import annotations

import collections
import re
import subprocess
from pathlib import Path

from workspace_paths import TENSOR_WALLPAPER_ROOT, WALLPAPER_DISTRIBUTION

ROOT = TENSOR_WALLPAPER_ROOT
DIST = WALLPAPER_DISTRIBUTION

EXES = {
    "x64": DIST / "wallpaper64.exe",
    "x86": DIST / "wallpaper32.exe",
}

METHODS = {
    "x64": {
        0x60: "DrawIndexed",
        0x68: "Draw",
    },
    "x86": {
        0x30: "DrawIndexed",
        0x34: "Draw",
    },
}

CLASSIFICATION = {
    "x64": {
        0x14005A10F: ("raw", "utility raw context [obj+0x80]; Draw(6, 0)"),
        0x14005A775: ("raw", "utility raw context [obj+0x80]; Draw([obj+0x118], 0)"),
        0x14005EAD7: ("raw", "utility raw context [obj+0x80]; Draw([obj+0x118], 0)"),
        0x14005C63B: ("d3dcompiler", "D3DCompile function pointer call; shader source/entrypoint/target args, not D3D Draw"),
        0x14005C6BB: ("d3dcompiler", "D3DCompile function pointer call; shader source/entrypoint/target args, not D3D Draw"),
        0x14005C90E: ("d3dcompiler", "D3DCompile function pointer call; shader source/entrypoint/target args, not D3D Draw"),
        0x14005CBA5: ("d3dcompiler", "D3D compiler/reflection wrapper call over ID3DBlob buffers, not DrawIndexed"),
        0x14005CC85: ("d3dcompiler", "D3D compiler/reflection wrapper call over ID3DBlob buffers, not Draw"),
        0x1400D5DA4: ("shader-compiler", "material shader compiler wrapper method; bytecode blob in/out, not DrawIndexed"),
        0x1400D5E1F: ("shader-compiler", "material shader compiler wrapper method; bytecode blob in/out, not Draw"),
        0x1400D68A1: ("shader-compiler", "material shader compiler wrapper method; stage bytecode output at +0xd8, not DrawIndexed"),
        0x1400D68E0: ("shader-compiler", "material shader compiler wrapper method; stage bytecode output at +0xe0, not Draw"),
        0x1400EA849: ("raw", "RT method 0x1400ea780; DrawIndexed after 0x140099f60"),
        0x1400EA85D: ("raw", "RT method 0x1400ea780; Draw after 0x140099f60"),
        0x1400EADBB: ("raw", "RT method 0x1400eacd0; DrawIndexed after 0x140099f60"),
        0x140030289: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140032143: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140032313: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x14007AE65: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140088C51: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x14008CFB5: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x1400F132A: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140116D19: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x1401007E0: ("property-wrapper", "custom property wrapper boolean setter at object+0x160, not Draw"),
        0x140100831: ("property-wrapper", "custom property wrapper integer/resource setter at object+0x160, not DrawIndexed"),
        0x14010125B: ("property-wrapper", "custom property wrapper integer/resource setter after QueryInterface-style call, not DrawIndexed"),
        0x140101271: ("property-wrapper", "custom property wrapper boolean setter after QueryInterface-style call, not Draw"),
        0x140113549: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401161C9: ("scene-object-method", "scene object type getter; return compared with 7, not DrawIndexed"),
        0x140121003: ("media-com", "Media Foundation/COM HRESULT-style call before WaitForSingleObject, not DrawIndexed"),
        0x140121045: ("media-com", "Media Foundation/COM HRESULT-style stop/close call, not DrawIndexed"),
        0x14012105D: ("media-com", "Media Foundation/COM HRESULT-style stop/close call, not Draw"),
        0x140123795: ("media-com", "Media Foundation/video COM reset call with zero args, not Draw"),
        0x1401238F2: ("media-com", "Media Foundation/video COM reset call with zero args, not Draw"),
        0x140123A5A: ("media-com", "Media Foundation/video COM open call with 0x8002 flags, not Draw"),
        0x140123DFE: ("media-com", "Media Foundation/video sample stream read method, not DrawIndexed"),
        0x140123FB9: ("media-com", "Media Foundation/video COM reset call on error path, not Draw"),
        0x14012452E: ("media-com", "Media Foundation/video media type iterator/query, not DrawIndexed"),
        0x140125168: ("media-com", "Media Foundation/video buffer lock/query with out params, not DrawIndexed"),
        0x14014D219: ("render-wrapper", "render-state wrapper [state+0x1518] helper/resource method, not DrawIndexed"),
        0x14014D23B: ("render-wrapper", "render-state wrapper [state+0x1518] helper/resource method, not Draw"),
        0x14014CE4E: ("render-wrapper", "render-state wrapper [state+0x1518] cache/resource method, not DrawIndexed"),
        0x14014CE70: ("render-wrapper", "render-state wrapper [state+0x1518] cache/resource method, not Draw"),
        0x140181749: ("scene-object-method", "scene object type getter; return compared with 7, not DrawIndexed"),
        0x140183299: ("scene-object-method", "scene object type getter; return compared with 5, not DrawIndexed"),
        0x1401832A7: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401832B9: ("scene-object-method", "scene object type getter; return compared with 5, not DrawIndexed"),
        0x1401881A3: ("scene-object-method", "scene object type getter; return compared with 1/4, not DrawIndexed"),
        0x14018A041: ("scene-object-method", "scene object type getter; return compared with 1/4/5, not DrawIndexed"),
        0x14018ADC8: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x14018ADF3: ("scene-object-method", "scene object type getter; return compared with 6, not DrawIndexed"),
        0x14018AE72: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x14018AE9D: ("scene-object-method", "scene object type getter; return compared with 6, not DrawIndexed"),
        0x14018AF02: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x14018AF2D: ("scene-object-method", "scene object type getter; return compared with 6, not DrawIndexed"),
        0x14018AFD9: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x14018AFF3: ("scene-object-method", "scene object type getter; return compared with 6, not DrawIndexed"),
        0x140208469: ("layer-custom", "scene/layer object vtable +0x68 bool-return, not D3D Draw"),
        0x14000CB7B: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14000CD98: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14000D2AF: ("streambuf-custom", "C++ text/stream buffer get/put facet method; 16-bit char path, not D3D DrawIndexed"),
        0x14000D3FB: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14000D61B: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14000E072: ("streambuf-custom", "locale/stream facet virtual method; returns 16-bit char, not D3D DrawIndexed"),
        0x14000EA37: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14000EBC7: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x14001004F: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x1400100A6: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x140012A3C: ("streambuf-custom", "C++ text/stream buffer flush/sync virtual call, not D3D Draw"),
        0x140012A93: ("streambuf-custom", "C++ text/stream buffer flush/sync virtual call, not D3D Draw"),
        0x140318627: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not D3D Draw"),
        0x140318680: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not D3D Draw"),
        0x14032283F: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not D3D Draw"),
        0x140327DFD: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not D3D Draw"),
        0x14034F80C: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034F846: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034F87C: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034F9C1: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034FA07: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034FA2E: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034FBC9: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034FC09: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x14034FC62: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + span descriptor, not D3D Draw"),
        0x140351024: ("stack-callback", "function pointer from [rsp+0x68] in glyph/math loop, not a D3D vtable"),
        0x140352BC9: ("font-raster", "glyph bitmap/raster callback; args are raster ctx + descriptor, not D3D Draw"),
        0x140056A85: ("resource-callback", "custom resource callback with string key/out params, not DrawIndexed"),
        0x1400690EB: ("image-helper", "custom image/layout helper with bbox out params and bool return, not DrawIndexed"),
        0x1400723AB: ("image-helper", "custom image/layout helper with rect out params and bool return, not DrawIndexed"),
        0x1400750C0: ("image-helper", "custom image/layout helper returns an object handle; fallback waits on UI/OS APIs, not Draw"),
        0x14007543A: ("image-helper", "custom image/layout helper returns an object handle; fallback waits on UI/OS APIs, not Draw"),
        0x140099844: ("dxgi-com", "IDXGISwapChain-style ResizeBuffers HRESULT path; args are width/height/format/flags, not Draw"),
        0x1400AB701: ("text-parser", "custom text/parser cursor method; return pointer checked for '#', not DrawIndexed"),
        0x1400D09C8: ("effect-wrapper", "material/effect wrapper vector method with pointer args, not DrawIndexed"),
        0x1400D1B93: ("effect-wrapper", "material/effect wrapper vector method with pointer args, not DrawIndexed"),
        0x1400EBE3D: ("effect-wrapper", "custom effect/target helper at object+0x158 with out params, not DrawIndexed"),
        0x1400EBE76: ("effect-wrapper", "custom effect/target helper at object+0x158 with resource arg, not Draw"),
        0x140111165: ("dxgi-com", "IDXGISwapChain-style ResizeBuffers HRESULT path; args are width/height/format/flags, not Draw"),
        0x14011F937: ("property-wrapper", "custom nested property setter using xmm1 float, not Draw"),
        0x14011FE41: ("dxgi-com", "IDXGISwapChain-style ResizeBuffers HRESULT path; args are width/height/format/flags, not Draw"),
        0x14012055B: ("media-com", "Media/video property setter with xmm1 playback value, not Draw"),
        0x1401217BB: ("media-com", "Media Foundation/video buffer lock/query with out params, not DrawIndexed"),
        0x140121E4A: ("media-com", "Media Foundation/COM stop/close HRESULT method, not DrawIndexed"),
        0x140121E79: ("media-com", "Media Foundation/COM stop/close HRESULT method, not DrawIndexed"),
        0x140121E8F: ("media-com", "Media Foundation/COM stop/close HRESULT method, not Draw"),
        0x14012A7C6: ("image-helper", "custom image decoder pixel-buffer query with width/height out params, not DrawIndexed"),
        0x14012A8E7: ("image-helper", "custom image decoder pixel-buffer release/update method, not Draw"),
        0x14013C1D9: ("com-query", "COM query/open call with HRESULT and out object, not Draw"),
        0x14013E8D5: ("com-query", "stack-owned COM wrapper cleanup callback, not Draw"),
        0x14014B705: ("effect-wrapper", "effect/vector wrapper method over float arrays, not DrawIndexed"),
        0x14014B934: ("effect-wrapper", "effect/vector wrapper method over float arrays, not DrawIndexed"),
        0x14014BABC: ("effect-wrapper", "effect/vector wrapper method over float arrays, not DrawIndexed"),
        0x14014BE44: ("effect-wrapper", "effect/vector wrapper method over float arrays, not DrawIndexed"),
        0x14014BFCC: ("effect-wrapper", "effect/vector wrapper method over float arrays, not DrawIndexed"),
        0x14018B2F9: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401D3BD3: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401D3BE1: ("scene-object-method", "scene object type getter; return compared with 4, not DrawIndexed"),
        0x1401D3C4B: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x1401D42F5: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401D461D: ("scene-object-method", "scene object type getter; return compared with 5, not DrawIndexed"),
        0x1401D462B: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401D4662: ("scene-object-method", "scene object type getter; return compared with 1/5, not DrawIndexed"),
        0x1401D6D59: ("scene-object-method", "scene object type getter; return compared with 1, not DrawIndexed"),
        0x1401DE3C0: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x1401DFCC2: ("scene-object-method", "scene object type getter; return compared with 8, not DrawIndexed"),
        0x1401DFF29: ("scene-object-method", "scene object type getter; return compared with 8, not DrawIndexed"),
        0x1401E8AB1: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x1401EAE13: ("scene-object-method", "scene object type getter; return range-checked against enum bits, not DrawIndexed"),
        0x1401ECD9D: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x1401FA6AC: ("property-wrapper", "custom property getter returning xmm0 float to caller out param, not DrawIndexed"),
        0x140213FDE: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x14021CE66: ("scene-callback", "scene/layer list callback with element count, not DrawIndexed"),
        0x14025FAFF: ("scene-object-method", "scene object visibility/update bool method; return tested as bool, not Draw"),
        0x14026C812: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x14027753E: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140277626: ("streambuf-custom", "C++ text/stream buffer sync/flush virtual call; -1 failure path, not Draw"),
        0x140343619: ("font-parser", "font parser function table callback with parser state args, not DrawIndexed"),
        0x140343FD6: ("font-parser", "font parser/object cleanup callback, not Draw"),
        0x14034504A: ("font-parser", "font parser function table callback with glyph/buffer args, not Draw"),
        0x14034507E: ("font-parser", "font parser function table callback with glyph/buffer args, not DrawIndexed"),
        0x140350E8C: ("font-raster", "glyph/font bitmap callback around 'bits' state, not Draw"),
        0x140352D88: ("font-raster", "glyph bitmap/raster callback; args are glyph ctx + descriptor, not D3D Draw"),
        0x14035FC2C: ("image-helper", "custom image/dimension helper over decoder callback table, not DrawIndexed"),
        0x14035FD74: ("image-helper", "custom image/dimension helper over decoder callback table, not Draw"),
        0x14035FFB8: ("image-helper", "custom image/dimension helper over decoder callback table, not Draw"),
        0x14039BFCA: ("image-helper", "image codec/reader callback with out buffer and dimensions, not DrawIndexed"),
        0x14039C01A: ("image-helper", "image codec/reader cleanup callback on allocation failure, not Draw"),
        0x14039C0E3: ("image-helper", "image codec/reader cleanup callback on fallback path, not Draw"),
        0x14039C113: ("image-helper", "image codec/reader cleanup callback on error path, not Draw"),
        0x14039C133: ("image-helper", "image codec/reader wrapper cleanup callback, not Draw"),
        0x14039E141: ("image-helper", "image/effect filter callback with six float parameters, not DrawIndexed"),
        0x14039E219: ("image-helper", "image/effect filter callback with six float parameters, not Draw"),
        0x1403BE5EF: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x1403C618E: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x1403CA886: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x1403CA9AE: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x1403CE938: ("image-helper", "image codec object attach/cleanup callback, not Draw"),
        0x1403D7896: ("image-helper", "image codec/dimension helper over decoder callback table, not DrawIndexed"),
        0x1403ED9E7: ("image-helper", "image/effect filter callback with sampled float parameters, not DrawIndexed"),
        0x1403EDC87: ("image-helper", "image/effect filter callback with sampled float parameters, not DrawIndexed"),
        0x1403EDF23: ("image-helper", "image/effect filter callback with sampled float parameters, not Draw"),
        0x1403EE1C3: ("image-helper", "image/effect filter callback with sampled float parameters, not Draw"),
        0x14040DBD9: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x14040DDC4: ("image-helper", "image/dimension helper over decoder callback table, not Draw"),
        0x140413DE9: ("image-helper", "image codec/reader callback with resource and out params, not Draw"),
    },
    "x86": {
        0x44AB34: ("raw", "utility raw context; Draw(6, 0)"),
        0x44AE5B: ("raw", "utility raw context; Draw([obj+0xa4], 0)"),
        0x4BB95F: ("raw", "RT method peer; DrawIndexed after 0x476e00"),
        0x4BB96B: ("raw", "RT method peer; Draw after 0x476e00"),
        0x4BBD4A: ("raw", "RT method peer; DrawIndexed after 0x476e00"),
        0x40B14E: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40B587: ("streambuf-custom", "C++ text/stream buffer get/put facet method; 16-bit char path, not DrawIndexed"),
        0x40B6D0: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40C1C9: ("streambuf-custom", "locale/stream facet virtual method; returns 16-bit char, not DrawIndexed"),
        0x40D53D: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40DB03: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40E2B0: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40E357: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x40ED60: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x41036A: ("streambuf-custom", "C++ text/stream buffer flush/sync virtual call, not D3D Draw"),
        0x428698: ("streambuf-custom", "C++ text/stream buffer flush/sync virtual call, not D3D Draw"),
        0x444849: ("app-callback", "app/config handler callback while iterating command nodes; nearby string 'logon', not DrawIndexed"),
        0x45EB26: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x44F3F0: ("win-callback", "Windows message/timer callback; return value drives a polling loop, not Draw"),
        0x468FE5: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x46997B: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x46CB2A: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x4ACA03: ("shader-compiler", "material shader compiler wrapper method; bytecode/blob args, not DrawIndexed"),
        0x4ACA39: ("shader-compiler", "material shader compiler wrapper method; bytecode/blob args, not Draw"),
        0x4BCEE7: ("image-helper", "custom image/dimension helper; result drives buffer allocation, not D3D Draw"),
        0x4C04CE: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x4C09FB: ("streambuf-custom", "C++ text/stream buffer overflow-style method; returns -1 on failure"),
        0x4CEABE: ("property-wrapper", "custom property wrapper bool setter; receiver at +0xfc, not Draw"),
        0x4CEB09: ("property-wrapper", "custom property wrapper int/resource setter; receiver at +0xfc, not DrawIndexed"),
        0x4CF24A: ("property-wrapper", "custom property wrapper int/resource setter; receiver at +0xfc, not DrawIndexed"),
        0x476B8F: ("dxgi-com", "DXGI/COM HRESULT-style call; checks 0x887a0005/0x887a0007, not Draw"),
        0x4E7C73: ("property-wrapper", "custom property wrapper float setter, not Draw"),
        0x4E8579: ("property-wrapper", "custom property wrapper float setter, not Draw"),
        0x4E91F9: ("media-com", "Media Foundation/COM query with out-params and HRESULT return, not DrawIndexed"),
        0x4EA44A: ("media-com", "Media Foundation/COM stop/flush HRESULT method, not DrawIndexed"),
        0x4EA480: ("media-com", "Media Foundation/COM stop/flush HRESULT method, not DrawIndexed"),
        0x4EA493: ("media-com", "Media Foundation/COM stop/flush HRESULT method, not Draw"),
        0x4EAB19: ("media-com", "Media Foundation/COM create/open call with flags 0x8002, not Draw"),
        0x4EAFA3: ("media-com", "Media Foundation/COM sample/query call with HRESULT return, not DrawIndexed"),
        0x4EB0C3: ("media-com", "Media Foundation/COM cleanup call with null args, not Draw"),
        0x4EB3BA: ("media-com", "Media Foundation/COM sample read call, not DrawIndexed"),
        0x4EBC99: ("media-com", "Media Foundation/COM query with out-params and HRESULT return, not DrawIndexed"),
        0x4EEC30: ("media-com", "Media Foundation/video frame callback after buffer copy, not Draw"),
        0x4FCA92: ("media-com", "Media Foundation/COM metadata/query call with out-params, not Draw"),
        0x4FE692: ("media-com", "Media Foundation/COM cleanup/release helper, not Draw"),
        0x52F8D6: ("scene-object-type", "scene object vtable type getter; return compared with 5/1, not DrawIndexed"),
        0x52F8E2: ("scene-object-type", "scene object vtable type getter; return compared with 1, not DrawIndexed"),
        0x52F8EE: ("scene-object-type", "scene object vtable type getter; return compared with 5, not DrawIndexed"),
        0x535C88: ("scene-object-type", "scene object vtable type getter; return compared with 1/4/5, not DrawIndexed"),
        0x53678E: ("scene-object-type", "scene object vtable type getter; return compared with 7, not DrawIndexed"),
        0x536B36: ("scene-object-type", "scene object vtable type getter; return compared with 1, not DrawIndexed"),
        0x56ABBF: ("scene-object-type", "scene object vtable type getter; return compared with 1, not DrawIndexed"),
        0x56ABCB: ("scene-object-type", "scene object vtable type getter; return compared with 4, not DrawIndexed"),
        0x56B150: ("scene-object-type", "scene object vtable type getter; return compared with 1, not DrawIndexed"),
        0x573468: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x5747C8: ("scene-object-type", "scene object vtable type getter; return compared with 8, not DrawIndexed"),
        0x57497D: ("scene-object-type", "scene object vtable type getter; return compared with 8, not DrawIndexed"),
        0x57CEC5: ("stack-callback", "function pointer from [esp+0x30], not a D3D vtable"),
        0x5896A3: ("scene-callback", "nested scene/property callback with one float argument, not Draw"),
        0x5A0B94: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x5EFA8F: ("scene-callback", "scene/layer lifecycle callback with object argument, not Draw"),
        0x6FCED3: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not DrawIndexed"),
        0x6FCF2B: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not Draw"),
        0x6FCFA4: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not Draw"),
        0x6FCFC4: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not Draw"),
        0x6FCFDF: ("font-parser", "PostScript/CFF parser virtual dispatch in font code, not Draw"),
        0x72AF33: ("font-raster", "glyph bitmap/raster callback; args are glyph ctx + descriptor, not D3D Draw"),
        0x769617: ("font-raster", "glyph bitmap/raster callback; args are glyph ctx + descriptor, not D3D Draw"),
    },
}

PATTERNS = {
    "x64": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*callq\s+\*0x([0-9a-f]+)\(%"
        r"(r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|10|11|12|13|14|15))\)"
    ),
    "x86": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*calll\s+\*0x([0-9a-f]+)\(%"
        r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
    ),
}


def disassemble(path: Path) -> str:
    return subprocess.run(
        ["llvm-objdump", "-d", str(path)],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


for arch, path in EXES.items():
    sites = []
    for line in disassemble(path).splitlines():
        match = PATTERNS[arch].match(line)
        if not match:
            continue
        addr = int(match.group(1), 16)
        offset = int(match.group(2), 16)
        reg = match.group(3)
        method = METHODS[arch].get(offset)
        if method is None:
            continue
        bucket, note = CLASSIFICATION[arch].get(addr, ("unclassified", "receiver not traced"))
        sites.append((addr, offset, reg, method, bucket, note))

    print(f"\n{arch} Draw/DrawIndexed-shaped call sites ({len(sites)}):")

    method_counts = collections.Counter(method for _, _, _, method, _, _ in sites)
    print("Methods:")
    for method in sorted(method_counts):
        print(f"  {method:12s} {method_counts[method]}")

    bucket_counts = collections.Counter(bucket for _, _, _, _, bucket, _ in sites)
    print("Buckets:")
    for bucket in sorted(bucket_counts):
        print(f"  {bucket:14s} {bucket_counts[bucket]}")

    print("Classified sites:")
    for addr, _, reg, method, bucket, note in sites:
        if bucket == "unclassified":
            continue
        print(f"  0x{addr:x} via %{reg:3s}  {method:12s}  {bucket:14s}  {note}")

    unclassified = [
        (addr, reg, method)
        for addr, _, reg, method, bucket, _ in sites
        if bucket == "unclassified"
    ]
    if unclassified:
        print("Unclassified sites:")
        for addr, reg, method in unclassified:
            print(f"  0x{addr:x} via %{reg:3s}  {method:12s}")
