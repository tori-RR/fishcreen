async (page) => {
  const checks = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  const mode = page.getByRole('combobox', { name: '运行模式', exact: true });
  const dim = page.getByRole('spinbutton', { name: '亮度变暗时间数值', exact: true });
  await dim.fill('0'); await dim.press('Tab');
  assert(await mode.inputValue() === 'Custom', '修改预设自动进入自定义');
  assert(await page.getByRole('slider', { name: '亮度变暗帧数', exact: true }).isDisabled(), '变暗0秒禁用帧数');
  await dim.fill('3'); await dim.press('Tab');
  assert(await dim.inputValue() === '3.0', '变暗时间上限3秒');
  assert(await page.getByRole('slider', { name: '亮度变暗帧数', exact: true }).isEnabled(), '有渐变时可调帧数');
  const restore = page.getByRole('spinbutton', { name: '亮度恢复时间数值', exact: true });
  await restore.fill('0'); await restore.press('Tab');
  assert(await page.getByRole('slider', { name: '亮度恢复帧数', exact: true }).isDisabled(), '恢复0秒禁用恢复帧数');
  await mode.selectOption('Movie');
  assert(await dim.inputValue() === '0.3', '观影固定预设不被编辑污染');
  assert(!(await page.getByRole('checkbox', { name: '黑幕渐变', exact: true }).isChecked()), '观影模式不渐入黑幕');
  await mode.selectOption('Custom');
  assert(await dim.inputValue() === '3.0', '切回自定义保留草稿');
  assert(await restore.inputValue() === '0.0', '自定义恢复时间保留');
  await page.evaluate(() => {
    window.__busyStates = [];
    const descriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'inert');
    Object.defineProperty(document.getElementById('settings-content'), 'inert', {
      get() { return descriptor.get.call(this); },
      set(value) { window.__busyStates.push(value); descriptor.set.call(this, value); },
      configurable: true,
    });
  });
  await page.getByRole('button', { name: '应用', exact: true }).click();
  assert(await page.getByText('设置已应用', { exact: true }).isVisible(), '应用成功且窗口不关闭');
  assert(await page.evaluate(() => window.__busyStates.join(',') === 'true,false'), '应用期间锁定编辑并在完成后解锁');
  await mode.selectOption('Daily');
  assert(await dim.inputValue() === '0.3', '日常预设保持0.3秒');
  assert(await restore.inputValue() === '0.5', '日常恢复默认0.5秒');
  await page.getByRole('combobox', { name: '界面外观', exact: true }).selectOption('Dark');
  await page.setViewportSize({ width: 760, height: 880 });
  await page.locator('main').evaluate(el => el.scrollTop = 0);
  await page.waitForTimeout(250);
  await page.screenshot({ path: '.impeccable/review/desktop.png' });
  assert(await page.locator('body').evaluate(el => el.scrollWidth <= innerWidth), '桌面无水平溢出');
  await page.locator('main').evaluate(el => el.scrollTop = el.scrollHeight);
  await page.screenshot({ path: '.impeccable/review/desktop-bottom.png' });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('main').evaluate(el => el.scrollTop = 0);
  await page.screenshot({ path: '.impeccable/review/mobile.png' });
  assert(await page.locator('body').evaluate(el => el.scrollWidth <= innerWidth), '窄窗口无水平溢出');
  await page.setViewportSize({ width: 760, height: 880 });
  await page.getByRole('combobox', { name: '界面外观', exact: true }).selectOption('Light');
  await page.locator('main').evaluate(el => el.scrollTop = 0);
  await page.waitForTimeout(250);
  assert(await page.getByRole('button', { name: '保存', exact: true }).evaluate(el => getComputedStyle(el).backgroundColor) === 'rgb(54, 107, 183)', '明亮主题主按钮颜色稳定');
  await page.screenshot({ path: '.impeccable/review/light.png' });
  return { checks, count: checks.length };
}
