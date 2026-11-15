import type { Profile, Settings } from './types';

export type NumericProfileKey = {
  [K in keyof Profile]: Profile[K] extends number ? K : never
}[keyof Profile];

export interface SliderSpec {
  key: NumericProfileKey;
  label: string;
  description: string;
  min: number;
  max: number;
  step: number;
  scale?: number;
  unit: string;
}

export const sliders: SliderSpec[] = [
  { key: 'leaveDelayMilliseconds', label: '鼠标离开后变暗', description: '离开所选屏幕后，等待多久开始调暗', min: .5, max: 10, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessFadeDurationMilliseconds', label: '亮度变暗时间', description: '0 秒为立即变暗', min: 0, max: 3, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessFadeFrames', label: '亮度变暗帧数', description: '帧数越高，亮度变化越细腻', min: 5, max: 60, step: 1, unit: '帧' },
  { key: 'brightnessFadeExponent', label: '曲线指数', description: '数值越高，前段变暗越快、后段越慢', min: 1, max: 4, step: .1, unit: '' },
  { key: 'brightnessRecoveryMilliseconds', label: '亮度恢复时间', description: '鼠标进入所选屏幕时恢复；0 秒为立即恢复', min: 0, max: 1, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessRecoveryFrames', label: '亮度恢复帧数', description: '恢复硬件亮度时的过渡帧数', min: 1, max: 30, step: 1, unit: '帧' },
  { key: 'idleBlackoutMilliseconds', label: '鼠标无动作后黑屏', description: '全局鼠标静止达到此时间后，开始显示黑幕', min: 5, max: 300, step: 1, scale: 1000, unit: '秒' },
  { key: 'blackoutRecoveryMilliseconds', label: '黑幕恢复时间', description: '鼠标活动后，黑幕恢复到完全透明的时间', min: .1, max: 5, step: .1, scale: 1000, unit: '秒' },
];

/** Quantize in displayed units before converting to the Rust integer unit. */
export function quantizeNumericInput(spec: SliderSpec, raw: string): number {
  if (raw.trim() === '') throw new Error(`${spec.label}不能为空`);
  const value = Number(raw);
  if (!Number.isFinite(value)) throw new Error(`${spec.label}需要有效数字`);
  const clamped = Math.max(spec.min, Math.min(spec.max, value));
  const steps = (clamped - spec.min) / spec.step;
  const stepped = spec.min + Math.round(steps + Number.EPSILON * Math.max(1, Math.abs(steps)) * 4) * spec.step;
  const displayValue = Math.max(spec.min, Math.min(spec.max, Number(stepped.toFixed(9))));
  return spec.key === 'brightnessFadeExponent'
    ? displayValue
    : Math.round(displayValue * (spec.scale ?? 1));
}

export function profileValidationError(value: Profile): { key: NumericProfileKey; message: string } | null {
  for (const spec of sliders) {
    const number = value[spec.key];
    const scale = spec.scale ?? 1;
    if (!Number.isFinite(number) || number < spec.min * scale || number > spec.max * scale ||
        (spec.key !== 'brightnessFadeExponent' && (!Number.isInteger(number) || number > 0xffffffff))) {
      return { key: spec.key, message: `${spec.label}的数值无效，请重新输入` };
    }
  }
  return null;
}

/** Backend inventory/state can refresh without overwriting an edited field. */
export function mergeSettingsDraft(current: Settings, incoming: Settings, edited: ReadonlySet<keyof Settings>): Settings {
  const merged = structuredClone(incoming);
  for (const key of edited) Object.assign(merged, { [key]: structuredClone(current[key]) });
  return merged;
}
