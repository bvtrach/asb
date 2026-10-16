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
  const state = { scale: 1, tx: 0, ty: 0, drag: null, pinch: null, pointers: new Map(), pinned: null, pinnedParts: new Set(), animation: null, suppressClickUntil: 0 };
  const viewBox = svg.viewBox.baseVal;
  const titles = new Map();

  const rawTitleOf = (element) => element.dataset.graphTitle || element.querySelector(':scope > title')?.textContent?.trim() || element.querySelector('title')?.textContent?.trim() || element.id || 'graph element';
  const visibleNameOf = (element) => {
    const text = [...element.querySelectorAll(':scope > text, :scope > g text')].map((item) => item.textContent.trim()).filter(Boolean).join(' · ');
    return text || rawTitleOf(element);
  };
  const titleOf = (element) => {
    const raw = rawTitleOf(element);
    if (!element.classList.contains('edge')) return visibleNameOf(element);
    const ends = raw.split('->').map((part) => titles.get(part.trim())).filter(Boolean);
    return ends.length === 2 ? `${visibleNameOf(ends[0])} → ${visibleNameOf(ends[1])}` : raw;
  };
  allElements.forEach((element) => {
    const title = rawTitleOf(element);
    titles.set(title, element);
    element.setAttribute('tabindex', '0');
    element.setAttribute('role', element.classList.contains('node') ? 'button' : 'img');
    element.dataset.graphTitle = title;
  });
  // Keep Graphviz titles in data attributes for navigation, but remove the
  // native SVG title popups so they cannot compete with the styled tooltip.
  svg.querySelectorAll('title').forEach((title) => title.remove());
  counts.textContent = `${nodes.length} nodes · ${edges.length} edges · ${clusters.length} modules`;
  canvas.style.width = `${viewBox.width}px`;
  canvas.style.height = `${viewBox.height}px`;
  svg.setAttribute('width', viewBox.width);
  svg.setAttribute('height', viewBox.height);

  function applyTransform() { canvas.style.transform = `translate3d(${state.tx}px, ${state.ty}px, 0) scale(${state.scale})`; }
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
    // Use the rendered rectangle instead of getBBox(). Graphviz wraps its
    // content in a translated root <g>, so raw SVG coordinates do not map
    // directly to the CSS transform applied by this viewer.
    const viewportRect = viewport.getBoundingClientRect();
    const elementRect = element.getBoundingClientRect();
    const currentX = elementRect.left - viewportRect.left + elementRect.width / 2;
    const currentY = elementRect.top - viewportRect.top + elementRect.height / 2;
    const graphX = (currentX - state.tx) / state.scale;
    const graphY = (currentY - state.ty) / state.scale;
    const targetScale = Math.max(.08, Math.min(2.5, state.scale));
    moveTo(targetScale, viewport.clientWidth / 2 - graphX * targetScale, viewport.clientHeight / 2 - graphY * targetScale, true);
  }
  function centerElements(elements) {
    const viewportRect = viewport.getBoundingClientRect();
    const rects = elements.map((element) => element.getBoundingClientRect()).filter((rect) => rect.width > 0 && rect.height > 0);
    if (!rects.length) return;
    const left = Math.min(...rects.map((rect) => rect.left - viewportRect.left));
    const top = Math.min(...rects.map((rect) => rect.top - viewportRect.top));
    const right = Math.max(...rects.map((rect) => rect.right - viewportRect.left));
    const bottom = Math.max(...rects.map((rect) => rect.bottom - viewportRect.top));
    const width = Math.max(1, right - left);
    const height = Math.max(1, bottom - top);
    const currentX = (left + right) / 2;
    const currentY = (top + bottom) / 2;
    const targetScale = Math.max(.05, Math.min(2.5, state.scale * Math.min(1.5, (viewport.clientWidth - 72) / width, (viewport.clientHeight - 72) / height)));
    const graphX = (currentX - state.tx) / state.scale;
    const graphY = (currentY - state.ty) / state.scale;
    moveTo(targetScale, viewport.clientWidth / 2 - graphX * targetScale, viewport.clientHeight / 2 - graphY * targetScale, true);
  }
  function edgeEnds(edge) {
    const parts = rawTitleOf(edge).split('->').map((part) => part.trim());
    if (parts.length !== 2) return [];
    return parts.map((part) => titles.get(part)).filter(Boolean);
  }
  function followEdge(edge, reverse = false) {
    const ends = edgeEnds(edge);
    const target = ends[reverse ? 0 : 1];
    if (target) {
      selection.textContent = titleOf(edge);
      centerElements([...relatedTo(edge), target]);
    } else {
      center(edge);
    }
  }
  function relatedTo(element) {
    const title = titleOf(element);
    const related = new Set([element]);
    if (element.classList.contains('edge')) {
      edgeEnds(element).forEach((endpoint) => related.add(endpoint));
      return related;
    }
    if (!element.classList.contains('node')) return related;
    edges.forEach((edge) => { if (edgeEnds(edge).includes(element)) { related.add(edge); edgeEnds(edge).forEach((endpoint) => related.add(endpoint)); } });
    return related;
  }
  function clearSelection() {
    state.pinnedParts.forEach((item) => item.classList.remove('is-pinned', 'is-hovered'));
    state.pinnedParts.clear();
    state.pinned = null;
    allElements.forEach((item) => item.classList.remove('is-dimmed', 'is-selected', 'is-related'));
    selection.textContent = 'Hover an element for details. Drag to pan; scroll to zoom.';
  }
  function select(element, reverseEdge = false) {
    if (state.pinned === element) { clearSelection(); return; }
    clearSelection();
    state.pinned = element;
    const related = relatedTo(element);
    const pinnedParts = [element, ...related.filter((item) => item.classList.contains('edge'))];
    state.pinnedParts = new Set(pinnedParts);
    state.pinnedParts.forEach((item) => item.classList.add('is-pinned', 'is-hovered'));
    selection.textContent = titleOf(element);
    if (element.classList.contains('edge')) followEdge(element, reverseEdge); else centerElements([...related]);
  }
  function showTooltip(element, event) {
    tooltip.textContent = titleOf(element);
    tooltip.style.display = 'block';
    tooltip.style.left = `${Math.min(event.clientX + 14, window.innerWidth - tooltip.offsetWidth - 12)}px`;
    tooltip.style.top = `${Math.min(event.clientY + 14, window.innerHeight - tooltip.offsetHeight - 12)}px`;
  }
  allElements.forEach((element) => {
    element.addEventListener('pointerenter', (event) => { element.classList.add('is-hovered'); showTooltip(element, event); });
    element.addEventListener('pointermove', (event) => { if (tooltip.style.display !== 'none') showTooltip(element, event); });
    element.addEventListener('pointerleave', () => { if (!element.classList.contains('is-pinned')) element.classList.remove('is-hovered'); tooltip.style.display = 'none'; });
    element.addEventListener('click', (event) => { if (state.suppressClickUntil > performance.now()) return; event.stopPropagation(); select(element, element.classList.contains('edge') && (event.ctrlKey || event.metaKey)); });
    element.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(element); } });
  });
  const pointerDistance = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
  const pointerMidpoint = (a, b) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
  function startPinch() {
    const points = [...state.pointers.values()];
    if (points.length < 2) return;
    const midpoint = pointerMidpoint(points[0], points[1]);
    const viewportRect = viewport.getBoundingClientRect();
    const startX = midpoint.x - viewportRect.left;
    const startY = midpoint.y - viewportRect.top;
    state.pinch = { distance: Math.max(1, pointerDistance(points[0], points[1])), midpoint, scale: state.scale, graphX: (startX - state.tx) / state.scale, graphY: (startY - state.ty) / state.scale, moved: false };
    state.drag = null;
  }
  viewport.addEventListener('pointerdown', (event) => { if (event.button !== 0) return; state.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY }); viewport.setPointerCapture(event.pointerId); if (state.pointers.size >= 2) { startPinch(); } else { state.drag = { x: event.clientX, y: event.clientY, tx: state.tx, ty: state.ty, moved: false }; } viewport.classList.add('dragging'); });
  viewport.addEventListener('pointermove', (event) => {
    if (!state.pointers.has(event.pointerId)) return;
    state.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (state.pointers.size >= 2) {
      if (!state.pinch) startPinch();
      const points = [...state.pointers.values()];
      const midpoint = pointerMidpoint(points[0], points[1]);
      const ratio = pointerDistance(points[0], points[1]) / state.pinch.distance;
      const nextScale = Math.max(.02, Math.min(4, state.pinch.scale * ratio));
      const viewportRect = viewport.getBoundingClientRect();
      const x = midpoint.x - viewportRect.left;
      const y = midpoint.y - viewportRect.top;
      state.scale = nextScale;
      state.tx = x - state.pinch.graphX * nextScale;
      state.ty = y - state.pinch.graphY * nextScale;
      state.pinch.moved = true;
      applyTransform();
      return;
    }
    if (!state.drag) return;
    const dx = event.clientX - state.drag.x;
    const dy = event.clientY - state.drag.y;
    if (Math.hypot(dx, dy) > 4) state.drag.moved = true;
    state.drag.nextTx = state.drag.tx + dx;
    state.drag.nextTy = state.drag.ty + dy;
    if (!state.drag.frame) state.drag.frame = requestAnimationFrame(() => { state.tx = state.drag?.nextTx ?? state.tx; state.ty = state.drag?.nextTy ?? state.ty; state.drag.frame = 0; applyTransform(); });
  });
  function finishPointer(event) {
    state.pointers.delete(event.pointerId);
    if (state.pinch) {
      if (state.pinch.moved) state.suppressClickUntil = performance.now() + 180;
      state.pinch = null;
      if (state.pointers.size === 1) {
        const point = [...state.pointers.values()][0];
        state.drag = { x: point.x, y: point.y, tx: state.tx, ty: state.ty, moved: true };
        return;
      }
    }
    if (state.pointers.size > 0) return;
    if (state.drag?.frame) cancelAnimationFrame(state.drag.frame);
    if (state.drag?.nextTx !== undefined) { state.tx = state.drag.nextTx; state.ty = state.drag.nextTy; applyTransform(); }
    if (state.drag?.moved) state.suppressClickUntil = performance.now() + 120;
    state.drag = null;
    viewport.classList.remove('dragging');
  }
  viewport.addEventListener('pointerup', finishPointer);
  viewport.addEventListener('pointercancel', finishPointer);
  viewport.addEventListener('click', (event) => { if (state.suppressClickUntil > performance.now()) return; if (event.target === viewport || event.target === canvas) clearSelection(); });
  viewport.addEventListener('wheel', (event) => { event.preventDefault(); const rect = viewport.getBoundingClientRect(); const x = event.clientX - rect.left; const y = event.clientY - rect.top; const next = Math.max(.02, Math.min(4, state.scale * Math.pow(1.0018, -event.deltaY))); state.tx = x - (x - state.tx) * next / state.scale; state.ty = y - (y - state.ty) * next / state.scale; state.scale = next; applyTransform(); }, { passive: false });
  document.getElementById('fit').addEventListener('click', () => { clearSelection(); fit(); });
  document.getElementById('reset').addEventListener('click', () => { clearSelection(); fit(false); });
  document.getElementById('zoom-in').addEventListener('click', () => { state.scale = Math.min(4, state.scale * 1.25); applyTransform(); });
  document.getElementById('zoom-out').addEventListener('click', () => { state.scale = Math.max(.02, state.scale / 1.25); applyTransform(); });
  search.addEventListener('input', () => { const query = search.value.trim().toLowerCase(); allElements.forEach((item) => item.classList.toggle('is-dimmed', Boolean(query) && !titleOf(item).toLowerCase().includes(query))); });
  window.addEventListener('resize', () => { if (!state.selected) fit(false); });
  fit(false);
})();
