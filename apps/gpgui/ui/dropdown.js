// Reusable custom dropdown. Builds a styled trigger in `mount` and a fixed-position
// popover appended to <body> (so it escapes any overflow/scroll container).
//
//   const dd = createDropdown(el, {
//     options: [{ value, label, sub? }, ...],
//     value: 'init', placeholder: 'Select…', onChange: (v) => {}
//   });
//   dd.value            // -> current value
//   dd.value = 'x'      // set value (no onChange fired)
//   dd.setOptions([...]) // replace options

(function (global) {
  const CHEV = '<svg class="dd-chev" width="12" height="8" viewBox="0 0 12 8" fill="none" stroke="#8a91a8" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M1 1.5l5 5 5-5"/></svg>';
  const CHECK = '<svg class="dd-check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12l5 5L20 6"/></svg>';
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  function createDropdown(mount, opts) {
    let options = opts.options || [];
    let value = opts.value != null ? opts.value : '';
    const placeholder = opts.placeholder || 'Select…';
    const onChange = opts.onChange;
    let open = false;

    const trigger = document.createElement('button');
    trigger.type = 'button';
    trigger.className = 'dd-trigger';

    const menu = document.createElement('div');
    menu.className = 'dd-menu';
    menu.hidden = true;
    document.body.appendChild(menu);

    function labelFor(v) {
      const o = options.find((o) => o.value === v);
      return o ? o.label : placeholder;
    }
    function renderTrigger() {
      const has = options.some((o) => o.value === value);
      trigger.innerHTML = '<span class="dd-label' + (has ? '' : ' ph') + '">' + esc(labelFor(value)) + '</span>' + CHEV;
    }
    function buildMenu() {
      menu.innerHTML = '';
      options.forEach((o) => {
        const b = document.createElement('button');
        b.type = 'button';
        b.className = 'dd-opt' + (o.value === value ? ' sel' : '');
        b.innerHTML =
          '<span class="dd-opt-main"><span class="dd-opt-l">' + esc(o.label) + '</span>' +
          (o.sub ? '<span class="dd-opt-s">' + esc(o.sub) + '</span>' : '') + '</span>' +
          (o.value === value ? CHECK : '');
        b.addEventListener('click', () => {
          value = o.value;
          close();
          renderTrigger();
          if (onChange) onChange(value);
        });
        menu.appendChild(b);
      });
    }
    function position() {
      const r = trigger.getBoundingClientRect();
      menu.style.top = (r.bottom + 6) + 'px';
      menu.style.left = r.left + 'px';
      menu.style.width = r.width + 'px';
    }
    function openMenu() {
      buildMenu();
      position();
      menu.hidden = false;
      open = true;
      trigger.classList.add('active');
    }
    function close() {
      menu.hidden = true;
      open = false;
      trigger.classList.remove('active');
    }

    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      open ? close() : openMenu();
    });
    document.addEventListener('mousedown', (e) => {
      if (open && !menu.contains(e.target) && !trigger.contains(e.target)) close();
    }, true);
    window.addEventListener('scroll', () => { if (open) close(); }, true);
    window.addEventListener('resize', () => { if (open) close(); });

    mount.appendChild(trigger);
    renderTrigger();

    return {
      get value() { return value; },
      set value(v) { value = v; renderTrigger(); },
      setOptions(o) { options = o || []; renderTrigger(); if (open) buildMenu(); },
      el: trigger,
    };
  }

  global.createDropdown = createDropdown;
})(window);
