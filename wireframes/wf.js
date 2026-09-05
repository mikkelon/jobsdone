// Wireframe helper: theme switching and screen navigation. Not product code.
(function () {
  const SCREENS = [
    ['index.html', 'Map & flows'],
    ['01-review.html', '01 Morning review: pile'],
    ['02-surfaced.html', '02 Morning review: surfaced'],
    ['03-today.html', '03 Today + backlog'],
    ['04-narrow.html', '04 Narrow tile'],
    ['05-task-states.html', '05 Task states'],
    ['06-dates.html', '06 Due, remind, waiting'],
    ['07-repeat.html', '07 Repeat schedule'],
    ['08-history.html', '08 History'],
    ['09-search.html', '09 Search'],
    ['10-scratchpad.html', '10 Scratchpad'],
    ['11-palette-help.html', '11 Palette & help'],
    ['12-empty.html', '12 Empty states'],
  ];
  const params = new URLSearchParams(location.search);
  const theme = params.get('theme') || 'wireframe';
  const T = window.OMARCHY_THEMES || {};
  if (theme !== 'wireframe' && T[theme]) {
    const t = T[theme], r = document.documentElement.style;
    for (const k of Object.keys(t)) if (k !== 'mode') r.setProperty('--' + k, t[k]);
    document.documentElement.dataset.mode = t.mode;
  }
  const here = location.pathname.split('/').pop() || 'index.html';
  const i = SCREENS.findIndex(s => s[0] === here);
  const q = theme === 'wireframe' ? '' : '?theme=' + theme;
  const nav = document.createElement('nav');
  nav.className = 'wf-nav';
  const opts = ['<option value="wireframe">wireframe (grey)</option>']
    .concat(Object.keys(T).map(k => `<option value="${k}" ${k === theme ? 'selected' : ''}>${k} (${T[k].mode})</option>`)).join('');
  nav.innerHTML = `
    <a href="index.html${q}">map</a>
    ${i > 0 ? `<a href="${SCREENS[i - 1][0]}${q}">‹ prev</a>` : '<span>‹ prev</span>'}
    <span class="screen">${SCREENS[i] ? SCREENS[i][1] : document.title}</span>
    ${i >= 0 && i < SCREENS.length - 1 ? `<a href="${SCREENS[i + 1][0]}${q}">next ›</a>` : '<span>next ›</span>'}
    <span class="spacer"></span>
    <span>Omarchy theme:</span> <select id="wf-theme">${opts}</select>`;
  document.body.prepend(nav);
  nav.querySelector('#wf-theme').addEventListener('change', e => {
    const v = e.target.value;
    location.search = v === 'wireframe' ? '' : '?theme=' + v;
  });
})();
