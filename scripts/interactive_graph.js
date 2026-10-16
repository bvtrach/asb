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
  const state = { scale: 1, tx: 0, ty: 0, drag: null, pinch: null, pointers: new Map(), pressTarget: null, pinned: null, pinnedParts: new Set(), hovered: null, animation: null, suppressClickUntil: 0 };
  const viewBox = svg.viewBox.baseVal;
  // Establish the first useful viewport synchronously, then yield one frame
  // before preparing metadata, listeners, and accessibility state. This lets
  // the browser paint the graph at its initial zoom without a long blank pause.
  canvas.style.width = `${viewBox.width}px`;
  canvas.style.height = `${viewBox.height}px`;
  svg.setAttribute('width', viewBox.width);
  svg.setAttribute('height', viewBox.height);
  const initialScale = Math.max(.02, Math.min(4, Math.min((viewport.clientWidth - 34) / viewBox.width, (viewport.clientHeight - 34) / viewBox.height)));
  state.scale = initialScale;
  state.tx = (viewport.clientWidth - viewBox.width * initialScale) / 2;
  state.ty = (viewport.clientHeight - viewBox.height * initialScale) / 2;
  canvas.style.transform = `translate3d(${state.tx}px, ${state.ty}px, 0) scale(${state.scale})`;
  requestAnimationFrame(() => initialize());
  return;

  function initialize() {
  const titles = new Map();
  const nameCache = new Map();
  const titleCache = new Map();
  const edgeEndsCache = new Map();
  const relatedCache = new Map();
  const hoverTimers = new WeakMap();
  let transformFrame = 0;
  let navigationTimer = 0;
  let tooltipFrame = 0;
  let tooltipEvent = null;
  let tooltipElement = null;

  const rawTitleOf = (element) => element.dataset.graphTitle || element.querySelector(':scope > title')?.textContent?.trim() || element.querySelector('title')?.textContent?.trim() || element.id || 'graph element';
  const visibleNameOf = (element) => {
    if (nameCache.has(element)) return nameCache.get(element);
    const text = [...element.querySelectorAll(':scope > text, :scope > g text')].map((item) => item.textContent.trim()).filter(Boolean).join(' · ');
    const name = text || rawTitleOf(element);
    nameCache.set(element, name);
    return name;
  };
  const titleOf = (element) => {
    if (titleCache.has(element)) return titleCache.get(element);
    const raw = rawTitleOf(element);
    if (!element.classList.contains('edge')) {
      const name = visibleNameOf(element);
      titleCache.set(element, name);
      return name;
    }
    const ends = raw.split('->').map((part) => titles.get(part.trim())).filter(Boolean);
    const name = ends.length === 2 ? `${visibleNameOf(ends[0])} → ${visibleNameOf(ends[1])}` : raw;
    titleCache.set(element, name);
    return name;
  };
  allElements.forEach((element) => {
    const title = rawTitleOf(element);
    titles.set(title, element);
    element.setAttribute('tabindex', '0');
    element.setAttribute('role', element.classList.contains('node') ? 'button' : 'img');
    element.dataset.graphTitle = title;
    if (element.classList.contains('edge')) {
      element.querySelectorAll(':scope > path').forEach((path) => {
        const hitbox = path.cloneNode(false);
        hitbox.classList.add('edge-hitbox');
        hitbox.removeAttribute('marker-start');
        hitbox.removeAttribute('marker-mid');
        hitbox.removeAttribute('marker-end');
        hitbox.setAttribute('stroke', 'transparent');
        hitbox.setAttribute('stroke-width', '10');
        hitbox.setAttribute('fill', 'none');
        hitbox.setAttribute('pointer-events', 'stroke');
        element.appendChild(hitbox);
      });
    }
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
  function scheduleTransform() {
    if (transformFrame) return;
    transformFrame = requestAnimationFrame(() => { transformFrame = 0; applyTransform(); });
  }
  function beginNavigation() {
    viewport.classList.add('navigating');
    clearTimeout(navigationTimer);
    navigationTimer = setTimeout(() => viewport.classList.remove('navigating'), 110);
  }
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
    if (edgeEndsCache.has(edge)) return edgeEndsCache.get(edge);
    const parts = rawTitleOf(edge).split('->').map((part) => part.trim());
    const ends = parts.length === 2 ? parts.map((part) => titles.get(part)).filter(Boolean) : [];
    edgeEndsCache.set(edge, ends);
    return ends;
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
    if (relatedCache.has(element)) return relatedCache.get(element);
    const related = new Set([element]);
    if (element.classList.contains('edge')) {
      edgeEnds(element).forEach((endpoint) => related.add(endpoint));
      relatedCache.set(element, related);
      return related;
    }
    if (!element.classList.contains('node')) { relatedCache.set(element, related); return related; }
    edges.forEach((edge) => { const ends = edgeEnds(edge); if (ends.includes(element)) { related.add(edge); ends.forEach((endpoint) => related.add(endpoint)); } });
    relatedCache.set(element, related);
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
    const pinnedParts = element.classList.contains('edge') ? [...related] : [element, ...related.filter((item) => item.classList.contains('edge'))];
    state.pinnedParts = new Set(pinnedParts);
    state.pinnedParts.forEach((item) => item.classList.add('is-pinned', 'is-hovered'));
    selection.textContent = titleOf(element);
    if (element.classList.contains('edge')) followEdge(element, reverseEdge); else centerElements([...related]);
  }
  function renderTooltip() {
    tooltipFrame = 0;
    if (!tooltipElement || !tooltipEvent) return;
    const event = tooltipEvent;
    tooltip.textContent = titleOf(tooltipElement);
    tooltip.style.display = 'block';
    // Keep positioning independent of offsetWidth/offsetHeight reads. Those
    // synchronous layout queries are expensive on the large SVG flow graphs.
    tooltip.style.left = `${Math.max(12, Math.min(event.clientX + 14, window.innerWidth - 372))}px`;
    tooltip.style.top = `${Math.max(12, Math.min(event.clientY + 14, window.innerHeight - 84))}px`;
  }
  function showTooltip(element, event) {
    tooltipElement = element;
    tooltipEvent = event;
    if (!tooltipFrame) tooltipFrame = requestAnimationFrame(renderTooltip);
  }
  allElements.forEach((element) => {
    element.addEventListener('click', (event) => {
      if (state.suppressClickUntil > performance.now()) return;
      event.stopPropagation();
      select(element, element.classList.contains('edge') && (event.ctrlKey || event.metaKey));
    });
    element.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(element); } });
  });
  const pointerDistance = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
  const pointerMidpoint = (a, b) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
  function graphElementFromTarget(target) {
    if (target?.closest) {
      const closest = target.closest('.node, .edge, .cluster');
      if (closest) return closest;
    }
    let current = target;
    while (current && current !== viewport) {
      if (current.classList?.contains('node') || current.classList?.contains('edge') || current.classList?.contains('cluster')) return current;
      current = current.parentElement;
    }
    return null;
  }
  function graphElementAtPoint(x, y) {
    const candidates = document.elementsFromPoint(x, y).map((item) => graphElementFromTarget(item)).filter(Boolean);
    return candidates.find((item) => item.classList.contains('node')) || candidates[0] || null;
  }
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
  viewport.addEventListener('pointerdown', (event) => { if (event.button !== 0) return; viewport.classList.remove('dragging'); state.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY }); state.pressTarget = graphElementFromTarget(event.target); viewport.setPointerCapture(event.pointerId); if (state.pointers.size >= 2) { state.pressTarget = null; startPinch(); viewport.classList.add('dragging'); } else { state.drag = { x: event.clientX, y: event.clientY, tx: state.tx, ty: state.ty, moved: false }; } });
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
      beginNavigation();
      scheduleTransform();
      return;
    }
    if (!state.drag) return;
    const dx = event.clientX - state.drag.x;
    const dy = event.clientY - state.drag.y;
    if (Math.hypot(dx, dy) > 4) { state.drag.moved = true; viewport.classList.add('dragging'); }
    state.drag.nextTx = state.drag.tx + dx;
    state.drag.nextTy = state.drag.ty + dy;
    if (!state.drag.frame) state.drag.frame = requestAnimationFrame(() => { state.tx = state.drag?.nextTx ?? state.tx; state.ty = state.drag?.nextTy ?? state.ty; state.drag.frame = 0; beginNavigation(); applyTransform(); });
  });
  function finishPointer(event) {
    const pointTarget = graphElementAtPoint(event.clientX, event.clientY);
    const pressTarget = pointTarget?.classList.contains('node') ? pointTarget : (state.pressTarget || pointTarget || graphElementFromTarget(event.target));
    const wasPinching = Boolean(state.pinch);
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
    const wasDragged = state.drag?.moved;
    if (!wasPinching && !wasDragged && pressTarget) {
      // Pointer capture retargets the browser click to the viewport. Activate
      // the original graph element explicitly so click and drag remain distinct.
      state.suppressClickUntil = performance.now() + 450;
      event.preventDefault();
      select(pressTarget, pressTarget.classList.contains('edge') && (event.ctrlKey || event.metaKey));
    } else if (wasDragged) state.suppressClickUntil = performance.now() + 120;
    state.drag = null;
    state.pressTarget = null;
    viewport.classList.remove('dragging');
  }
  viewport.addEventListener('pointerup', finishPointer);
  viewport.addEventListener('pointercancel', finishPointer);
  viewport.addEventListener('lostpointercapture', () => { if (state.pointers.size === 0) { state.drag = null; state.pinch = null; state.pressTarget = null; viewport.classList.remove('dragging'); } });
  viewport.addEventListener('pointermove', (event) => {
    if (viewport.classList.contains('dragging')) return;
    const hovered = graphElementFromTarget(event.target) || graphElementAtPoint(event.clientX, event.clientY);
    if (hovered) {
      if (state.hovered !== hovered) {
        if (state.hovered && !state.hovered.classList.contains('is-pinned')) state.hovered.classList.remove('is-hovered');
        state.hovered = hovered;
        state.hovered.classList.add('is-hovered');
      }
      showTooltip(hovered, event);
    } else {
      if (state.hovered && !state.hovered.classList.contains('is-pinned')) state.hovered.classList.remove('is-hovered');
      state.hovered = null;
      tooltipElement = null;
      tooltipEvent = null;
      tooltip.style.display = 'none';
    }
  });
  viewport.addEventListener('pointerleave', () => {
    if (state.hovered && !state.hovered.classList.contains('is-pinned')) state.hovered.classList.remove('is-hovered');
    state.hovered = null;
    tooltipElement = null;
    tooltipEvent = null;
    tooltip.style.display = 'none';
  });
  viewport.addEventListener('click', (event) => {
    if (state.suppressClickUntil > performance.now()) return;
    if (event.target === viewport || event.target === canvas) clearSelection();
  });
  viewport.addEventListener('wheel', (event) => { event.preventDefault(); const rect = viewport.getBoundingClientRect(); const x = event.clientX - rect.left; const y = event.clientY - rect.top; const next = Math.max(.02, Math.min(4, state.scale * Math.pow(1.0018, -event.deltaY))); state.tx = x - (x - state.tx) * next / state.scale; state.ty = y - (y - state.ty) * next / state.scale; state.scale = next; beginNavigation(); scheduleTransform(); }, { passive: false });
  document.getElementById('fit').addEventListener('click', () => { clearSelection(); fit(); });
  document.getElementById('reset').addEventListener('click', () => { clearSelection(); fit(false); });
  document.getElementById('zoom-in').addEventListener('click', () => { state.scale = Math.min(4, state.scale * 1.25); applyTransform(); });
  document.getElementById('zoom-out').addEventListener('click', () => { state.scale = Math.max(.02, state.scale / 1.25); applyTransform(); });
  search.addEventListener('input', () => { const query = search.value.trim().toLowerCase(); allElements.forEach((item) => item.classList.toggle('is-dimmed', Boolean(query) && !titleOf(item).toLowerCase().includes(query))); });
  window.addEventListener('resize', () => { if (!state.selected) fit(false); });
  fit(false);
  }
})();
