(() => {
  const viewport = document.getElementById('viewport');
  const canvas = document.getElementById('canvas');
  const svg = canvas.querySelector('svg');
  const tooltip = document.getElementById('tooltip');
  const selection = document.getElementById('selection');
  const counts = document.getElementById('counts');
  const search = document.getElementById('search');
  const nodes = [...canvas.querySelectorAll('.node')];
  const edges = [...canvas.querySelectorAll('.edge')];
  const clusters = [...canvas.querySelectorAll('.cluster')];
  const allElements = [...nodes, ...edges, ...clusters];
  const state = { scale: 1, tx: 0, ty: 0, drag: null, selected: null, animation: null };
  const viewBox = svg.viewBox.baseVal;
  const titles = new Map();

  const titleOf = (element) => element.querySelector(':scope > title')?.textContent?.trim() || element.querySelector('title')?.textContent?.trim() || element.id || 'graph element';
  allElements.forEach((element) => {
    const title = titleOf(element);
    titles.set(title, element);
    element.setAttribute('tabindex', '0');
    element.setAttribute('role', element.classList.contains('node') ? 'button' : 'img');
    element.dataset.graphTitle = title;
  });
  counts.textContent = `${nodes.length} nodes · ${edges.length} edges · ${clusters.length} modules`;
  canvas.style.width = `${viewBox.width}px`;
  canvas.style.height = `${viewBox.height}px`;
  svg.setAttribute('width', viewBox.width);
  svg.setAttribute('height', viewBox.height);

  function applyTransform() { canvas.style.transform = `translate(${state.tx}px, ${state.ty}px) scale(${state.scale})`; }
  function fit(animated = true) {
    const width = viewport.clientWidth;
    const height = viewport.clientHeight;
    const margin = 34;
    const next = Math.max(.02, Math.min(4, Math.min((width - margin) / viewBox.width, (height - margin) / viewBox.height)));
    moveTo(next, (width - viewBox.width * next) / 2, (height - viewBox.height * next) / 2, animated);
  }
  function moveTo(scale, tx, ty, animated = true) {
    if (!animated) { state.scale = scale; state.tx = tx; state.ty = ty; applyTransform(); return; }
    const from = { scale: state.scale, tx: state.tx, ty: state.ty };
    const started = performance.now();
    cancelAnimationFrame(state.animation);
    function frame(now) {
      const p = Math.min(1, (now - started) / 360);
      const eased = 1 - Math.pow(1 - p, 3);
      state.scale = from.scale + (scale - from.scale) * eased;
      state.tx = from.tx + (tx - from.tx) * eased;
      state.ty = from.ty + (ty - from.ty) * eased;
      applyTransform();
      if (p < 1) state.animation = requestAnimationFrame(frame);
    }
    state.animation = requestAnimationFrame(frame);
  }
  function center(element) {
    const box = element.getBBox();
    const targetScale = Math.max(.08, Math.min(2.5, Math.min(viewport.clientWidth / Math.max(box.width * 4, 420), viewport.clientHeight / Math.max(box.height * 4, 260))));
    moveTo(targetScale, viewport.clientWidth / 2 - (box.x + box.width / 2) * targetScale, viewport.clientHeight / 2 - (box.y + box.height / 2) * targetScale, true);
  }
  function edgeEnds(edge) {
    const parts = titleOf(edge).split('->').map((part) => part.trim());
    if (parts.length !== 2) return [];
    return parts.map((part) => titles.get(part)).filter(Boolean);
  }
  function followEdge(edge, reverse = false) {
    const ends = edgeEnds(edge);
    const target = ends[reverse ? 0 : 1];
    if (target) {
      selection.textContent = `${titleOf(edge)} → ${titleOf(target)}`;
      center(target);
    } else {
      center(edge);
    }
  }
  function relatedTo(element) {
    const title = titleOf(element);
    const related = new Set([element]);
    if (!element.classList.contains('node')) return related;
    edges.forEach((edge) => { const text = titleOf(edge); if (text.includes(title) || text.includes(element.id)) related.add(edge); });
    return related;
  }
  function select(element, reverseEdge = false) {
    state.selected = element;
    const related = relatedTo(element);
    allElements.forEach((item) => item.classList.toggle('is-dimmed', element.classList.contains('node') && !related.has(item)));
    allElements.forEach((item) => item.classList.toggle('is-selected', item === element));
    related.forEach((item) => item.classList.add('is-related'));
    selection.textContent = titleOf(element);
    if (element.classList.contains('edge')) followEdge(element, reverseEdge); else center(element);
  }
  function showTooltip(element, event) {
    tooltip.textContent = titleOf(element);
    tooltip.style.display = 'block';
    tooltip.style.left = `${Math.min(event.clientX + 14, window.innerWidth - tooltip.offsetWidth - 12)}px`;
    tooltip.style.top = `${Math.min(event.clientY + 14, window.innerHeight - tooltip.offsetHeight - 12)}px`;
  }
  function clearSelection() { state.selected = null; allElements.forEach((item) => item.classList.remove('is-dimmed', 'is-selected', 'is-related')); selection.textContent = 'Hover an element for details. Drag to pan; scroll to zoom.'; }
  allElements.forEach((element) => {
    element.addEventListener('pointerenter', (event) => { element.classList.add('is-hovered'); showTooltip(element, event); });
    element.addEventListener('pointermove', (event) => { if (tooltip.style.display !== 'none') showTooltip(element, event); });
    element.addEventListener('pointerleave', () => { element.classList.remove('is-hovered'); tooltip.style.display = 'none'; });
    element.addEventListener('click', (event) => { event.stopPropagation(); select(element, element.classList.contains('edge') && (event.ctrlKey || event.metaKey)); });
    element.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(element); } });
  });
  viewport.addEventListener('pointerdown', (event) => { if (event.button !== 0 || event.target.closest('.node, .edge, .cluster')) return; state.drag = { x: event.clientX, y: event.clientY, tx: state.tx, ty: state.ty }; viewport.classList.add('dragging'); viewport.setPointerCapture(event.pointerId); });
  viewport.addEventListener('pointermove', (event) => { if (!state.drag) return; state.tx = state.drag.tx + event.clientX - state.drag.x; state.ty = state.drag.ty + event.clientY - state.drag.y; applyTransform(); });
  viewport.addEventListener('pointerup', () => { state.drag = null; viewport.classList.remove('dragging'); });
  viewport.addEventListener('click', (event) => { if (event.target === viewport || event.target === canvas) clearSelection(); });
  viewport.addEventListener('wheel', (event) => { event.preventDefault(); const rect = viewport.getBoundingClientRect(); const x = event.clientX - rect.left; const y = event.clientY - rect.top; const next = Math.max(.02, Math.min(4, state.scale * Math.pow(1.0018, -event.deltaY))); state.tx = x - (x - state.tx) * next / state.scale; state.ty = y - (y - state.ty) * next / state.scale; state.scale = next; applyTransform(); }, { passive: false });
  document.getElementById('fit').addEventListener('click', () => { clearSelection(); fit(); });
  document.getElementById('reset').addEventListener('click', () => { clearSelection(); fit(false); });
  document.getElementById('zoom-in').addEventListener('click', () => { state.scale = Math.min(4, state.scale * 1.25); applyTransform(); });
  document.getElementById('zoom-out').addEventListener('click', () => { state.scale = Math.max(.02, state.scale / 1.25); applyTransform(); });
  search.addEventListener('input', () => { const query = search.value.trim().toLowerCase(); allElements.forEach((item) => item.classList.toggle('is-dimmed', Boolean(query) && !titleOf(item).toLowerCase().includes(query))); });
  window.addEventListener('resize', () => { if (!state.selected) fit(false); });
  fit(false);
})();
