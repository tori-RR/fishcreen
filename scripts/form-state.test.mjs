import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

const source = fs.readFileSync(new URL('../ui/form-state.ts', import.meta.url), 'utf8');
const exports = {};
vm.runInNewContext(ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exports, structuredClone });
const { sliders, quantizeNumericInput, profileValidationError, mergeSettingsDraft } = exports;
let assertions = 0;
function equal(actual, expected, label) { assert.equal(actual, expected, label); assertions++; }
function throws(run, label) { assert.throws(run, undefined, label); assertions++; }
const spec = key => sliders.find(item => item.key === key);

equal(quantizeNumericInput(spec('brightnessFadeFrames'), '5.5'), 6, 'fractional frames are quantized');
equal(quantizeNumericInput(spec('brightnessRecoveryMilliseconds'), '0.3333'), 300, 'seconds quantized before integer conversion');
equal(quantizeNumericInput(spec('leaveDelayMilliseconds'), '0.65'), 700, 'min-based steps round consistently');
equal(quantizeNumericInput(spec('blackoutRecoveryMilliseconds'), '0.15'), 200, 'decimal half-step is not rounded down by binary noise');
equal(quantizeNumericInput(spec('brightnessFadeExponent'), '2.44'), 2.4, 'only curve exponent remains floating point');
equal(quantizeNumericInput(spec('brightnessFadeDurationMilliseconds'), '1e-1'), 100, 'scientific notation uses same normalization');
equal(quantizeNumericInput(spec('brightnessFadeFrames'), '-99'), 5, 'minimum is bounded');
equal(quantizeNumericInput(spec('brightnessFadeFrames'), '999'), 60, 'maximum is bounded');
throws(() => quantizeNumericInput(spec('brightnessFadeFrames'), ''), 'empty does not silently become zero');
throws(() => quantizeNumericInput(spec('brightnessFadeFrames'), '   '), 'whitespace is invalid');
throws(() => quantizeNumericInput(spec('brightnessFadeFrames'), 'NaN'), 'NaN is invalid');
throws(() => quantizeNumericInput(spec('brightnessFadeFrames'), 'Infinity'), 'infinity is invalid');

const profile = Object.fromEntries(sliders.map(s => [s.key, quantizeNumericInput(s, String(s.min))]));
profile.brightnessFadeEnabled = true;
profile.blackoutFadeEnabled = true;
equal(profileValidationError(profile), null, 'normalized profile matches integer IPC contract');
for (const s of sliders) {
  for (const input of [s.min, s.max, (s.min + s.max) / 2, s.min - 100, s.max + 100]) {
    const value = quantizeNumericInput(s, String(input));
    equal(Number.isFinite(value), true, `${s.key} finite`);
    if (s.key !== 'brightnessFadeExponent') equal(Number.isInteger(value), true, `${s.key} integer u32`);
    equal(profileValidationError({ ...profile, [s.key]: value }), null, `${s.key} contract boundary`);
  }
}
equal(profileValidationError({ ...profile, brightnessFadeFrames: 5.5 })?.key, 'brightnessFadeFrames', 'strict integer contract rejects float');
equal(profileValidationError({ ...profile, brightnessRecoveryMilliseconds: 333.3 })?.key, 'brightnessRecoveryMilliseconds', 'strict millisecond contract rejects float');
equal(profileValidationError({ ...profile, brightnessFadeExponent: Infinity })?.key, 'brightnessFadeExponent', 'non-finite exponent rejected');
const settings = { targetStableId: 'screen-one', mode: 'Daily', themeMode: 'Dark', showActiveWindow: false, customProfile: profile };
const edited = { ...structuredClone(settings), targetStableId: 'edited-screen', themeMode: 'Light' };
const incoming = { ...structuredClone(settings), mode: 'Movie', targetStableId: 'external-screen', showActiveWindow: true };
const merged = mergeSettingsDraft(edited, incoming, new Set(['targetStableId', 'themeMode']));
equal(merged.targetStableId, 'edited-screen', 'target draft preserved');
equal(merged.themeMode, 'Light', 'theme draft preserved');
equal(merged.mode, 'Movie', 'untouched external mode synchronized');
equal(merged.showActiveWindow, true, 'untouched global option synchronized');
merged.customProfile.brightnessFadeFrames = 99;
equal(incoming.customProfile.brightnessFadeFrames, 5, 'merged profile never aliases backend state');
console.log(`form-state: ${assertions} assertions passed`);
