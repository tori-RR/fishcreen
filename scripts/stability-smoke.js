// Run last in an isolated Playwright CLI session against the Vite preview.
// The real Tauri JS SDK is used with a page-local IPC stub; no hardware or
// registry operation is possible from this fixture.
async (page) => {
  const checks = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  await page.setViewportSize({ width: 480, height: 880 });
  await page.addInitScript(() => {
    const profile = {
      leaveDelayMilliseconds: 1500, idleBlackoutMilliseconds: 30000,
      blackoutRecoveryMilliseconds: 500, brightnessRecoveryMilliseconds: 500,
      brightnessRecoveryFrames: 5, brightnessFadeDurationMilliseconds: 300,
      brightnessFadeEnabled: true, blackoutFadeEnabled: true,
      brightnessFadeFrames: 30, brightnessFadeExponent: 2.4,
    };
    const originalId = String.raw`\\?\DISPLAY#DELA0E7#FIXTURE_A#{GUID}`;
    window.__stability = {
      calls: [],
      state: {
        settings: { targetStableId: originalId, mode: 'Daily', themeMode: 'Dark', showActiveWindow: false, customProfile: profile },
        monitors: [{ stableId: originalId, name: 'Fixture original', gdiName: 'DISPLAY1', bounds: { left: 0, top: 0, right: 1440, bottom: 2560 }, primary: false }],
        status: '隔离 IPC 测试', brightnessDimmed: false, overlayOpacity: 0,
        startupEnabled: false, version: '1.2.1', shell: 'Tauri', recoveryRequiresConfirmation: null,
      },
    };
    window.isTauri = true;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      const fixture = window.__stability;
      if (command === 'get_state') return structuredClone(fixture.state);
      fixture.calls.push({ command, args: structuredClone(args) });
      if (command === 'apply_settings') {
        for (const [key, value] of Object.entries(args.settings.customProfile)) {
          if (typeof value === 'number' && (key !== 'brightnessFadeExponent' && !Number.isInteger(value) || !Number.isFinite(value))) throw new Error(`Invalid Rust numeric field: ${key}`);
        }
        fixture.state.settings = structuredClone(args.settings);
        if (args.startupEnabled !== null && args.startupEnabled !== undefined) fixture.state.startupEnabled = args.startupEnabled;
      } else if (command === 'confirm_recovery_target') {
        if (!fixture.state.monitors.some(item => item.stableId === args.target)) throw new Error('Disconnected fixture');
        fixture.state.recoveryRequiresConfirmation = null;
      } else if (!['close_settings', 'restore_brightness'].includes(command)) {
        throw new Error(`Unexpected command: ${command}`);
      }
      return structuredClone(fixture.state);
    } };
    // Accelerate only the production two-second poll, not UI events or promises.
    const timeout = window.setTimeout.bind(window);
    window.setTimeout = (callback, delay, ...args) => timeout(callback, delay === 2000 ? 100 : delay, ...args);
  });
  await page.reload();
  await page.waitForFunction(() => document.getElementById('live-status')?.textContent === '隔离 IPC 测试');
  assert(await page.locator('#recovery-binding').isHidden(), '没有旧恢复记录时确认区域保持隐藏');
  assert(await page.locator('#target option').count() === 1, '真实DOM初始化显示器列表');

  await page.locator('#theme').selectOption('Light');
  await page.evaluate(() => {
    const fixture = window.__stability;
    fixture.state.startupEnabled = true;
    fixture.state.monitors.push({ ...structuredClone(fixture.state.monitors[0]), stableId: String.raw`\\?\DISPLAY#DELA0E7#FIXTURE_B#{GUID}`, name: 'Fixture identical' });
  });
  await page.waitForFunction(() => document.getElementById('startup').checked && document.getElementById('target').options.length === 2);
  assert(await page.locator('#theme').inputValue() === 'Light', '热插拔与外部启动项变化不丢主题草稿');
  await page.locator('#apply').click();
  await page.waitForFunction(() => document.getElementById('apply').disabled && window.__stability.calls.some(item => item.command === 'apply_settings'));
  assert(await page.evaluate(() => window.__stability.calls.find(item => item.command === 'apply_settings').args.startupEnabled === null), '真实SDK提交未编辑启动项为null');
  assert(await page.evaluate(() => window.__stability.state.startupEnabled), '保存主题不会关闭外部启用的启动项');

  await page.locator('#brightnessFadeFrames-number').fill('5.5');
  await page.locator('#brightnessFadeFrames-number').press('Tab');
  assert(await page.locator('#brightnessFadeFrames-number').inputValue() === '6', '真实number输入将5.5帧量化为6帧');
  await page.locator('#brightnessFadeDurationMilliseconds-number').fill('0.3333');
  await page.locator('#brightnessFadeDurationMilliseconds-number').press('Tab');
  assert(await page.locator('#brightnessFadeDurationMilliseconds-number').inputValue() === '0.3', '时间显示与量化后的底层值一致');
  await page.locator('#apply').click();
  await page.waitForFunction(() => window.__stability.calls.filter(item => item.command === 'apply_settings').length === 2);
  const submitted = await page.evaluate(() => window.__stability.calls.filter(item => item.command === 'apply_settings').at(-1).args.settings.customProfile);
  assert(submitted.brightnessFadeFrames === 6 && submitted.brightnessFadeDurationMilliseconds === 300, '真实SDK边界只收到整数帧与毫秒');

  await page.locator('#brightnessFadeFrames-number').fill('');
  await page.locator('#save').click();
  assert(await page.locator('#brightnessFadeFrames-number').getAttribute('aria-invalid') === 'true', '空值触发可访问错误状态');
  assert(await page.locator('#brightnessFadeFrames-number').evaluate(input => input === document.activeElement), '提交失败后焦点定位首个非法输入');
  assert(await page.evaluate(() => window.__stability.calls.filter(item => item.command === 'apply_settings').length) === 2, '非法输入在IPC前阻止提交');
  await page.locator('#brightnessFadeFrames-number').fill('12');
  await page.locator('#brightnessFadeFrames-number').press('Tab');

  await page.evaluate(() => { window.__stability.state.monitors = []; });
  await page.waitForFunction(() => document.getElementById('monitor-detail').textContent.includes('暂停'));
  assert(await page.locator('#target option').count() === 1, '断连时保留草稿目标的缺失项');
  assert(await page.locator('#brightnessFadeFrames-number').inputValue() === '12', '设备断连不丢自定义数值草稿');
  await page.evaluate(() => {
    const fixture = window.__stability;
    fixture.state.monitors = [{ stableId: fixture.state.settings.targetStableId, name: 'Fixture original', gdiName: 'DISPLAY1', bounds: { left: 0, top: 0, right: 1440, bottom: 2560 }, primary: false }];
    fixture.state.recoveryRequiresConfirmation = 'DELA0E7';
  });
  await page.locator('#recovery-binding').waitFor({ state: 'visible' });
  await page.locator('#confirm-recovery').click();
  assert(await page.locator('#recovery-dialog').evaluate(dialog => dialog.open), '旧记录恢复使用原生模态二次确认');
  const dialogBounds = await page.locator('#recovery-dialog').boundingBox();
  assert(dialogBounds && dialogBounds.x >= 0 && dialogBounds.y >= 0 && dialogBounds.x + dialogBounds.width <= 480 && dialogBounds.y + dialogBounds.height <= 880, '恢复确认弹窗完整位于默认窄窗口内');
  await page.screenshot({ path: 'output/playwright/recovery-confirmation.png' });
  await page.getByRole('button', { name: '取消', exact: true }).last().click();
  assert(await page.evaluate(() => !window.__stability.calls.some(item => item.command === 'confirm_recovery_target')), '取消确认不会触发恢复命令');
  await page.locator('#confirm-recovery').click();
  await page.getByRole('button', { name: '确认并恢复', exact: true }).click();
  await page.locator('#recovery-binding').waitFor({ state: 'hidden' });
  assert(await page.evaluate(() => window.__stability.calls.filter(item => item.command === 'confirm_recovery_target').length === 1), '显式确认仅发送一次恢复请求');
  assert(await page.evaluate(() => window.__stability.calls.filter(item => item.command === 'apply_settings').length) === 2, '恢复确认不应用其他未保存草稿');
  assert(await page.locator('#brightnessFadeFrames-number').inputValue() === '12' && await page.locator('#apply').isEnabled(), '恢复完成仍保留未保存的自定义编辑');
  await page.screenshot({ path: 'output/playwright/stability-regression.png' });
  return { count: checks.length, checks };
}
