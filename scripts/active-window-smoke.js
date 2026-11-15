async (page) => {
  const checks = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  const mode = page.getByRole('combobox', { name: '运行模式', exact: true });
  const active = page.getByRole('checkbox', { name: '显示活跃窗口', exact: true });
  const originalMode = await mode.inputValue();
  const originalActive = await active.isChecked();
  const originalViewport = page.viewportSize();
  assert(!originalActive, '旧版与首次启动默认完全覆盖');
  try {
    await mode.selectOption('Daily');
    await page.locator('label[for="showActiveWindow"]').click();
    assert(await active.isChecked(), '文字标签可开启显示活跃窗口');
    assert(await mode.inputValue() === 'Daily', '全局覆盖偏好不切换日常预设');
    assert(await page.getByRole('button', { name: '应用', exact: true }).isEnabled(), '修改覆盖偏好进入待应用状态');
    for (const name of ['Movie', 'Custom', 'Off', 'Daily']) {
      await mode.selectOption(name);
      assert(await active.isChecked(), `${name}: 模式切换保留全局覆盖偏好`);
    }
    await mode.selectOption('Movie');
    await page.getByRole('button', { name: '应用', exact: true }).click();
    assert(await active.isChecked(), '应用并重新渲染后保留开关状态');
    assert(await mode.inputValue() === 'Movie', '应用覆盖偏好不污染观影模式');
    await active.focus();
    await active.press('Space');
    assert(!(await active.isChecked()), '键盘空格可切回完全覆盖');
    assert(await mode.inputValue() === 'Movie', '键盘修改也不切换到自定义');
    const semantics = await active.evaluate(input => ({
      native: input.type === 'checkbox' && !input.hasAttribute('role'),
      description: document.getElementById(input.getAttribute('aria-describedby'))?.textContent,
      hit: (() => { const box = input.closest('label').getBoundingClientRect(); return { width: box.width, height: box.height }; })(),
    }));
    assert(semantics.native, '保留原生checkbox语义');
    assert(semantics.description?.includes('关闭时完全覆盖'), '覆盖行为说明与开关关联');
    assert(semantics.hit.width >= 44 && semantics.hit.height >= 44, '开关点击区域至少44x44');
    await page.setViewportSize({ width: 480, height: 880 });
    await active.scrollIntoViewIfNeeded();
    assert(await page.locator('body').evaluate(el => el.scrollWidth <= innerWidth), '默认窄窗下开关无横向溢出');
    await page.screenshot({ path: 'output/playwright/active-window-setting.png' });
  } finally {
    if (await active.isChecked() !== originalActive) await page.locator('#showActiveWindow + span').click();
    await mode.selectOption(originalMode);
    await page.getByRole('button', { name: '应用', exact: true }).click();
    if (originalViewport) await page.setViewportSize(originalViewport);
  }
  return { count: checks.length, checks };
}
