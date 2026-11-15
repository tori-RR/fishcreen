async (page) => {
  const checks = [];
  const measurements = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  const theme = page.getByRole('combobox', { name: '界面外观', exact: true });
  const originalTheme = await theme.inputValue();
  const mode = page.getByRole('combobox', { name: '运行模式', exact: true });
  const originalMode = await mode.inputValue();
  const originalViewport = page.viewportSize();
  const structure = await page.evaluate(() => {
    const status = document.getElementById('live-status');
    const general = [...document.querySelectorAll('.setting-section')].find(section => section.querySelector('h2')?.textContent.trim() === '常规');
    return {
      removedSubtitle: !document.body.textContent.includes('让副屏安静一点'),
      version: document.getElementById('version').textContent.trim(),
      singleStatus: document.querySelectorAll('#live-status').length === 1,
      statusInHeader: !!status.closest('header') && !status.closest('main'),
      politeStatus: status.getAttribute('role') === 'status' || status.getAttribute('aria-live') === 'polite' || !!status.closest('[role="status"], [aria-live="polite"]'),
      mainHasNoStatus: !document.querySelector('main .status-strip, main #live-status, main [role="status"]'),
      generalHints: general ? general.querySelectorAll('.setting-copy p').length : -1,
      generalCheckboxSemantics: document.getElementById('startup')?.matches('input[type="checkbox"]') && !document.getElementById('startup')?.hasAttribute('role'),
      brokenDescriptions: [...document.querySelectorAll('[aria-describedby]')].flatMap(el => el.getAttribute('aria-describedby').trim().split(/\s+/).filter(id => !document.getElementById(id)).map(id => `${el.id}:${id}`)),
      descriptions: [...document.querySelectorAll('main .setting-copy p, main .section-note')].map(el => el.textContent.trim()),
    };
  });
  assert(structure.removedSubtitle, '移除让副屏安静一点副标题');
  assert(/^\d+\.\d+\.\d+$/.test(structure.version), '版本仅显示x.y.z且不含Tauri');
  assert(structure.singleStatus && structure.statusInHeader && structure.politeStatus, '唯一实时状态位于页头且保留礼貌播报');
  assert(structure.mainHasNoStatus, '参数区不再包含状态条');
  assert(structure.generalHints === 0, '常规两项不再显示提示文案');
  assert(structure.generalCheckboxSemantics, '开机自动启动保留原生checkbox语义');
  assert(structure.brokenDescriptions.length === 0, `aria-describedby无悬挂引用: ${structure.brokenDescriptions.join(', ')}`);
  const preservedDescriptions = [
    '关闭时完全覆盖；开启时不遮挡当前前台窗口',
    '使用前快后慢的曲线降低硬件亮度',
    '离开所选屏幕后，等待多久开始调暗',
    '0 秒为立即变暗',
    '帧数越高，亮度变化越细腻',
    '数值越高，前段变暗越快、后段越慢',
    '鼠标进入所选屏幕时恢复；0 秒为立即恢复',
    '恢复硬件亮度时的过渡帧数',
    '全局鼠标静止达到此时间后，开始显示黑幕',
    '日常与自定义模式下，用 5 秒渐变进入黑屏',
    '鼠标活动后，黑幕恢复到完全透明的时间',
    '硬件亮度通过 DDC 控制。帧数较高时，显示器响应可能延长实际过渡时间。',
  ];
  for (const description of preservedDescriptions) {
    assert(structure.descriptions.includes(description), `保留参数说明: ${description}`);
  }
  const auditTheme = () => {
    const rgba = value => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error(`无法解析颜色: ${value}`);
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const over = (foreground, background) => {
      const alpha = foreground[3] + background[3] * (1 - foreground[3]);
      return [...foreground.slice(0, 3).map((value, index) => (value * foreground[3] + background[index] * background[3] * (1 - foreground[3])) / (alpha || 1)), alpha];
    };
    const luminance = color => color.slice(0, 3).map(value => value / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
    const contrast = (a, b) => { const values = [luminance(a), luminance(b)].sort((x, y) => y - x); return (values[0] + .05) / (values[1] + .05); };
    const backgroundFor = element => {
      const ancestors = [];
      for (let current = element; current; current = current.parentElement) ancestors.unshift(current);
      return ancestors.reduce((background, current) => over(rgba(getComputedStyle(current).backgroundColor), background), [255, 255, 255, 1]);
    };
    const text = [...document.querySelectorAll('header *, main *, footer *')].filter(element => {
      const style = getComputedStyle(element);
      if (style.display === 'none' || style.visibility !== 'visible' || !element.getClientRects().length || element.closest('.sr-only, [hidden], [disabled], .is-disabled .slider-control')) return false;
      return [...element.childNodes].some(node => node.nodeType === Node.TEXT_NODE && node.textContent.trim()) || element.matches('input:not([type="range"]):not([type="checkbox"]), select');
    }).map(element => {
      const style = getComputedStyle(element);
      const background = backgroundFor(element);
      const color = over(rgba(style.color), background);
      const largeText = parseFloat(style.fontSize) >= 24 || (parseFloat(style.fontSize) >= 18.667 && Number(style.fontWeight) >= 700);
      return { element: element.id || element.tagName.toLowerCase(), text: element.matches('input, select') ? element.value : [...element.childNodes].filter(node => node.nodeType === Node.TEXT_NODE).map(node => node.textContent.trim()).join(' ').slice(0, 100), color: style.color, background, ratio: contrast(color, background), minimum: largeText ? 3 : 4.5 };
    });
    const root = getComputedStyle(document.documentElement);
    const save = getComputedStyle(document.getElementById('save'));
    return { theme: document.documentElement.dataset.theme, background: root.backgroundColor, accent: save.backgroundColor, text, failures: text.filter(entry => entry.ratio < entry.minimum) };
  };
  const switchStyle = id => {
    const input = document.getElementById(id);
    const track = input.nextElementSibling;
    const thumb = getComputedStyle(track, '::after');
    const channels = value => value.match(/[\d.]+/g).slice(0, 3).map(Number);
    const luminance = color => channels(color).map(value => value / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
    const trackColor = getComputedStyle(track).backgroundColor;
    const values = [luminance(trackColor), luminance(thumb.backgroundColor)].sort((a, b) => b - a);
    return { checked: input.checked, type: input.type, role: input.getAttribute('role'), background: trackColor, thumb: thumb.backgroundColor, x: thumb.transform === 'none' ? 0 : new DOMMatrixReadOnly(thumb.transform).m41, contrast: (values[0] + .05) / (values[1] + .05) };
  };
  try {
    await page.setViewportSize({ width: 760, height: 880 });
    for (const name of ['Dark', 'Light']) {
      await theme.selectOption(name);
      await page.waitForTimeout(200);
      const colors = await page.evaluate(auditTheme);
      assert(colors.theme === name.toLowerCase(), `${name}: 明确应用对应主题`);
      assert(colors.accent === (name === 'Dark' ? 'rgb(229, 229, 229)' : 'rgb(43, 43, 43)'), `${name}: 主按钮使用指定黑灰强调色`);
      if (name === 'Dark') assert(colors.background === 'rgb(16, 16, 16)', 'Dark: 背景使用#101010');
      assert(colors.text.length >= 30, `${name}: 覆盖页头参数区及页脚的文本对比度`);
      assert(colors.failures.length === 0, `${name}: 可用文本满足WCAG AA: ${JSON.stringify(colors.failures)}`);
      const switches = [];
      for (const id of ['brightnessFadeEnabled', 'blackoutFadeEnabled', 'showActiveWindow', 'startup']) {
        const input = page.locator(`#${id}`);
        const original = await input.isChecked();
        const track = page.locator(`#${id} + span`);
        const states = [];
        try {
          for (const checked of [false, true]) {
            if (await input.isChecked() !== checked) await track.click();
            await page.waitForTimeout(170);
            const visual = await page.evaluate(switchStyle, id);
            assert(visual.checked === checked && visual.type === 'checkbox' && visual.role === null, `${name}/${id}/${checked}: 原生checkbox状态正确`);
            assert(visual.contrast >= 3, `${name}/${id}/${checked}: 拇指与轨道对比度至少3:1 (${visual.contrast.toFixed(2)})`);
            states.push(visual);
          }
        } finally {
          if (await input.isChecked() !== original) await track.click();
        }
        assert(states[0].background !== states[1].background && states[1].x - states[0].x >= 12, `${name}/${id}: 开关同时用轨道明暗及拇指位置区分`);
        switches.push({ id, states });
      }
      measurements.push({ theme: name, colors, switches });
    }
  } finally {
    await mode.selectOption(originalMode);
    await theme.selectOption(originalTheme);
    if (originalViewport) await page.setViewportSize(originalViewport);
  }
  return { count: checks.length, checks, measurements };
}
