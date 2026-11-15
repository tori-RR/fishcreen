async (page) => {
  const checks = [];
  const measurements = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  const scenarios = [
    { name: 'desktop', width: 760, height: 880, zoom: 1 },
    { name: 'minimum', width: 540, height: 520, zoom: 1 },
    { name: 'narrow', width: 390, height: 844, zoom: 1 },
    { name: 'zoom125', width: 760, height: 880, zoom: 1.25 },
    { name: 'zoom150', width: 760, height: 880, zoom: 1.5 },
  ];
  const readLayout = () => {
    const main = document.querySelector('main');
    const header = document.querySelector('header').getBoundingClientRect();
    const footer = document.querySelector('footer').getBoundingClientRect();
    const status = document.getElementById('live-status').getBoundingClientRect();
    const restore = document.getElementById('restore').getBoundingClientRect();
    return {
      rootClient: document.documentElement.clientHeight,
      rootScroll: document.documentElement.scrollHeight,
      rootTop: document.scrollingElement.scrollTop,
      horizontalOverflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      headerTop: header.top, footerTop: footer.top, footerBottom: footer.bottom,
      headerBottom: header.bottom,
      statusTop: status.top, statusBottom: status.bottom,
      statusLeft: status.left, statusRight: status.right,
      statusVisible: status.width > 0 && status.height > 0,
      statusOverflow: document.getElementById('live-status').scrollWidth > document.getElementById('live-status').clientWidth + 1,
      restoreLeft: restore.left, restoreTop: restore.top, restoreBottom: restore.bottom,
      mainTop: main.getBoundingClientRect().top,
      mainBottom: main.getBoundingClientRect().bottom,
      mainScrollTop: main.scrollTop,
      scrollers: [...document.querySelectorAll('body *')].filter(el => {
        const style = getComputedStyle(el);
        return /auto|scroll/.test(style.overflowY) && el.clientHeight > 1 && el.scrollHeight > el.clientHeight + 1;
      }).map(el => el.id || el.tagName),
      labelsContained: [...document.querySelectorAll('main .sr-only')].every(el => el.offsetParent === main),
    };
  };
  for (const scenario of scenarios) {
    await page.setViewportSize({ width: scenario.width, height: scenario.height });
    await page.evaluate(zoom => { document.documentElement.style.zoom = String(zoom); }, scenario.zoom);
    await page.getByRole('combobox', { name: '界面外观', exact: true }).selectOption('Dark');
    await page.locator('main').evaluate(el => el.scrollTop = 0);
    const top = await page.evaluate(readLayout);
    const key = scenario.name;
    assert(top.rootScroll <= top.rootClient + 1 && top.rootTop === 0, `${key}: 外层无纵向滚动范围`);
    assert(!top.horizontalOverflow, `${key}: 无横向溢出`);
    assert(top.statusVisible && top.statusTop >= top.headerTop - 1 && top.statusBottom <= top.headerBottom + 1 && top.statusBottom <= top.mainTop + 1, `${key}: 首屏状态完整位于页头`);
    assert(top.scrollers.join(',') === 'settings-content', `${key}: 只有参数区滚动`);
    assert(top.labelsContained, `${key}: 隐藏标签被内容区包含`);
    await page.getByRole('button', { name: '保存', exact: true }).focus();
    await page.getByRole('combobox', { name: '界面外观', exact: true }).focus();
    const focused = await page.getByRole('combobox', { name: '界面外观', exact: true }).boundingBox();
    const focusedLayout = await page.evaluate(readLayout);
    assert(focused && focused.y >= focusedLayout.mainTop - 1 && focused.y + focused.height <= focusedLayout.mainBottom + 1, `${key}: 键盘聚焦末尾控件可见`);
    await page.locator('main').evaluate(el => el.scrollTop = el.scrollHeight);
    const bottom = await page.evaluate(readLayout);
    assert(Math.abs(top.headerTop - bottom.headerTop) < 1 && Math.abs(top.footerTop - bottom.footerTop) < 1 && bottom.rootTop === 0, `${key}: 滚到底后页头与按钮栏固定`);
    assert(Math.abs(top.statusTop - bottom.statusTop) < 1 && Math.abs(top.statusBottom - bottom.statusBottom) < 1, `${key}: 状态跟随页头固定而非参数区滚动`);
    assert(Math.abs(bottom.footerBottom - scenario.height) < 1, `${key}: 底部按钮贴合窗口底部`);
    measurements.push({ scenario: key, top, bottom });
    if (['desktop', 'minimum', 'zoom125'].includes(key)) {
      await page.screenshot({ path: `output/playwright/scroll-${key}-bottom.png` });
      await page.locator('main').evaluate(el => el.scrollTop = 0);
      await page.screenshot({ path: `output/playwright/scroll-${key}-top.png` });
    }
  }
  await page.evaluate(() => document.documentElement.style.removeProperty('zoom'));
  await page.setViewportSize({ width: 760, height: 880 });
  await page.getByRole('combobox', { name: '界面外观', exact: true }).selectOption('Light');
  await page.locator('main').evaluate(el => el.scrollTop = el.scrollHeight);
  await page.waitForTimeout(200);
  const light = await page.evaluate(readLayout);
  assert(light.rootScroll <= light.rootClient + 1 && light.scrollers.join(',') === 'settings-content', 'light: 明亮主题同样只滚动参数区');
  await page.screenshot({ path: 'output/playwright/scroll-light-bottom.png' });
  const originalStatus = await page.locator('#live-status').textContent();
  try {
    await page.locator('#live-status').evaluate(el => {
      el.textContent = '正在恢复显示器亮度，等待显示器响应 · '.repeat(8) + 'DisplayConnectionPending'.repeat(8);
    });
    for (const theme of ['Dark', 'Light']) {
      await page.getByRole('combobox', { name: '界面外观', exact: true }).selectOption(theme);
      for (const width of [390, 540, 760]) {
        await page.setViewportSize({ width, height: width === 540 ? 520 : 844 });
        await page.locator('main').evaluate(el => el.scrollTop = 0);
        const longStatus = await page.evaluate(readLayout);
        const key = `${theme.toLowerCase()}-${width}-long-status`;
        assert(!longStatus.horizontalOverflow && !longStatus.statusOverflow && longStatus.statusLeft >= 0 && longStatus.statusRight <= width + 1, `${key}: 长中文及连续英文状态无横向溢出`);
        assert(longStatus.statusVisible && longStatus.statusTop >= longStatus.headerTop - 1 && longStatus.statusBottom <= longStatus.headerBottom + 1 && longStatus.statusBottom <= longStatus.mainTop + 1, `${key}: 长状态保持在页头`);
        const statusAboveRestore = longStatus.statusBottom <= longStatus.restoreTop + 1;
        const statusBelowRestore = longStatus.statusTop >= longStatus.restoreBottom - 1;
        assert(statusAboveRestore || statusBelowRestore || longStatus.statusRight <= longStatus.restoreLeft + 1, `${key}: 长状态不覆盖立即恢复按钮`);
        assert(longStatus.rootScroll <= longStatus.rootClient + 1 && longStatus.rootTop === 0 && longStatus.scrollers.join(',') === 'settings-content', `${key}: 长状态下仍只有参数区滚动`);
        measurements.push({ scenario: key, top: longStatus });
      }
    }
  } finally {
    await page.locator('#live-status').evaluate((el, value) => { el.textContent = value; }, originalStatus);
    await page.setViewportSize({ width: 760, height: 880 });
  }
  return { count: checks.length, checks, measurements };
}
