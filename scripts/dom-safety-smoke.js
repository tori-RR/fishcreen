// F08: raw identities and labels must remain opaque data in both Option paths.
async (page) => {
  const checks = [];
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  await page.addInitScript(() => {
    window.isTauri = true;
    window.__domSafety = {
      settings: { targetStableId: 'ordinary', mode: 'Daily', themeMode: 'Dark', showActiveWindow: false, customProfile: {
        leaveDelayMilliseconds: 1500, idleBlackoutMilliseconds: 30000, blackoutRecoveryMilliseconds: 500,
        brightnessRecoveryMilliseconds: 500, brightnessRecoveryFrames: 5, brightnessFadeDurationMilliseconds: 300,
        brightnessFadeEnabled: true, blackoutFadeEnabled: true, brightnessFadeFrames: 30, brightnessFadeExponent: 2.4,
      } },
      monitors: [{ stableId: 'ordinary', name: 'Ordinary monitor', primary: false, gdiName: 'DISPLAY1', bounds: { left: 0, top: 0, right: 1440, bottom: 2560 } }],
      status: 'DOM safety fixture', version: '1.2.1', shell: 'Tauri', startupEnabled: false,
      brightnessDimmed: false, overlayOpacity: 0, recoveryRequiresConfirmation: null,
    };
    window.__TAURI_INTERNALS__ = { invoke: async command => {
      if (command !== 'get_state') throw new Error(`Unexpected mutation: ${command}`);
      return structuredClone(window.__domSafety);
    } };
    const timeout = window.setTimeout.bind(window);
    window.setTimeout = (callback, delay, ...args) => timeout(callback, delay === 2000 ? 50 : delay, ...args);
  });
  await page.reload();
  await page.waitForFunction(() => document.getElementById('live-status')?.textContent === 'DOM safety fixture');
  const cases = [
    ['raw-double-quote', 'screen-1" data-f08="sentinel', 'Panel <b data-f08="name">literal</b>'],
    ['literal-entities', 'screen-&quot;-&#34;-&amp;', '屏幕 &quot; &#34; &amp;'],
    ['opaque-windows-id', String.raw`\\?\DISPLAY#DELA0E7#INSTANCE#{GUID}`, 'DELL 副屏 & "主机"'],
    ['literal-markup', `screen'><option data-f08='extra'>`, '<img data-f08="image" src="not-a-resource">'],
  ];
  for (const [name, identity, label] of cases) {
    for (const connected of [true, false]) {
      await page.evaluate(({ identity, label, connected }) => {
        const fixture = window.__domSafety;
        fixture.settings.targetStableId = identity;
        fixture.monitors = connected ? [{ stableId: identity, name: label, primary: false, gdiName: 'DISPLAY1', bounds: { left: 0, top: 0, right: 1440, bottom: 2560 } }] : [];
      }, { identity, label, connected });
      await page.waitForFunction(({ identity, connected }) => {
        const target = document.getElementById('target');
        return target?.value === identity && document.getElementById('monitor-detail').textContent.includes(connected ? '1440' : '未连接');
      }, { identity, connected });
      const observed = await page.locator('#target').evaluate(select => ({
        value: select.value,
        values: [...select.options].map(option => option.value),
        texts: [...select.options].map(option => option.textContent),
        attributes: [...select.options].flatMap(option => [...option.attributes].map(attribute => attribute.name)),
        descendants: select.querySelectorAll('img, b, script, [data-f08]').length,
      }));
      const key = `${name}/${connected ? 'connected' : 'missing'}`;
      assert(observed.values.length === 1 && observed.value === identity && observed.values[0] === identity, `${key}: identity remains exact opaque data`);
      assert(observed.attributes.every(attribute => attribute === 'value') && observed.descendants === 0, `${key}: no injected attributes or elements`);
      assert(connected ? observed.texts[0] === label : observed.texts[0].includes('未连接'), `${key}: literal label or fixed fallback is preserved`);
    }
  }
  // Harmless negative control: the old text-escape-in-attribute pattern must
  // produce the sentinel attribute, proving the trigger is meaningful.
  const oldPatternInjects = await page.evaluate(() => {
    const text = document.createElement('span');
    text.textContent = 'screen-1" data-f08="sentinel';
    const select = document.createElement('select');
    select.innerHTML = `<option value="${text.innerHTML}">ordinary</option>`;
    return select.options[0].getAttribute('data-f08') === 'sentinel';
  });
  assert(oldPatternInjects, 'negative control reproduces the original harmless attribute injection');
  return { count: checks.length, checks };
}
