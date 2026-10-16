(() => {
  const viewport = document.getElementById('viewport');
  const canvas = document.getElementById('canvas');
  const svg = canvas.querySelector('svg');
  const tooltip = document.getElementById('tooltip');
  const selection = document.getElementById('selection');
  const search = document.getElementById('search');
  const nodes = [...canvas.querySelectorAll('.node')];
  const edges = [...canvas.querySelectorAll('.edge')];
  const clusters = [...canvas.querySelectorAll('.cluster')];
  const elements = [...nodes, ...edges, ...clusters];
  const viewBox = svg.viewBox.baseVal;

  canvas.style.width = `${viewBox.width}px`;
  canvas.style.height = `${viewBox.height}px`;
  svg.setAttribute('width', viewBox.width);
  svg.setAttribute('height', viewBox.height);
  const firstScale = Math.max(.02, Math.min(4, Math.min((viewport.clientWidth - 34) / viewBox.width, (viewport.clientHeight - 34) / viewBox.height)));
  const state = {
    scale: firstScale,
    tx: (viewport.clientWidth - viewBox.width * firstScale) / 2,
    ty: (viewport.clientHeight - viewBox.height * firstScale) / 2,
    hover: null, pinned: null, pinnedParts: new Set(), pointers: new Map(),
    gesture: null, pressTarget: null, transformFrame: 0, animation: 0,
    suppressClickUntil: 0, tooltipFrame: 0, tooltipElement: null,
    tooltipEvent: null, hoverRect: null,
  };
  canvas.style.transform = `translate3d(${state.tx}px, ${state.ty}px, 0) scale(${state.scale})`;
  requestAnimationFrame(initialize);
  return;

  function initialize() {
    const titles = new Map();
    const names = new Map();
    const edgeEndsCache = new Map();
    const relatedCache = new Map();
    const rawTitle = (element) => element.dataset.graphTitle || element.querySelector(':scope > title')?.textContent?.trim() || element.querySelector('title')?.textContent?.trim() || element.id || 'graph element';
    const visibleName = (element) => {
      if (names.has(element)) return names.get(element);
      const text = [...element.querySelectorAll(':scope > text, :scope > g text')].map((item) => item.textContent.trim()).filter(Boolean).join(' · ');
      const name = text || rawTitle(element);
      names.set(element, name);
      return name;
    };
    const titleOf = (element) => {
      if (!element) return 'graph element';
      if (!element.classList.contains('edge')) return visibleName(element);
      const ends = rawTitle(element).split('->').map((part) => titles.get(part.trim())).filter(Boolean);
      return ends.length === 2 ? `${visibleName(ends[0])} → ${visibleName(ends[1])}` : rawTitle(element);
    };
    const graphAncestor = (target) => {
      let item = target;
      while (item && item !== viewport) {
        if (item.classList?.contains('node') || item.classList?.contains('edge') || item.classList?.contains('cluster')) return item;
        item = item.parentElement;
      }
      return null;
    };
    const edgeEnds = (edge) => {
      if (edgeEndsCache.has(edge)) return edgeEndsCache.get(edge);
      const parts = rawTitle(edge).split('->').map((part) => part.trim());
      const result = parts.length === 2 ? parts.map((part) => titles.get(part)).filter(Boolean) : [];
      edgeEndsCache.set(edge, result);
      return result;
    };
    const relatedTo = (element) => {
      if (relatedCache.has(element)) return relatedCache.get(element);
      const result = new Set([element]);
      if (element.classList.contains('edge')) edgeEnds(element).forEach((end) => result.add(end));
      else if (element.classList.contains('node')) edges.forEach((edge) => {
        const ends = edgeEnds(edge);
        if (ends.includes(element)) { result.add(edge); ends.forEach((end) => result.add(end)); }
      });
      relatedCache.set(element, result);
      return result;
    };
    elements.forEach((element) => {
      const title = rawTitle(element);
      titles.set(title, element);
      element.dataset.graphTitle = title;
      element.setAttribute('tabindex', '0');
      element.setAttribute('role', element.classList.contains('node') ? 'button' : 'img');
      if (element.classList.contains('edge')) element.querySelectorAll(':scope > path').forEach((path) => {
        const hit = path.cloneNode(false);
        hit.classList.add('edge-hitbox');
        hit.removeAttribute('marker-start'); hit.removeAttribute('marker-mid'); hit.removeAttribute('marker-end');
        hit.setAttribute('stroke', 'transparent'); hit.setAttribute('stroke-width', '12'); hit.setAttribute('fill', 'none'); hit.setAttribute('pointer-events', 'bounding-box');
        element.appendChild(hit);
      });
    });
    svg.querySelectorAll('title').forEach((title) => title.remove());
    document.getElementById('counts').textContent = `${nodes.length} nodes · ${edges.length} edges · ${clusters.length} modules`;

    function applyTransform() { canvas.style.transform = `translate3d(${state.tx}px, ${state.ty}px, 0) scale(${state.scale})`; }
    function scheduleTransform() { if (!state.transformFrame) state.transformFrame = requestAnimationFrame(() => { state.transformFrame = 0; applyTransform(); }); }
    function moveTo(scale, tx, ty, animated = true) {
      cancelAnimationFrame(state.animation);
      if (!animated) { state.scale = scale; state.tx = tx; state.ty = ty; applyTransform(); return; }
      const start = { scale: state.scale, tx: state.tx, ty: state.ty }, started = performance.now();
      const frame = (now) => {
        const p = Math.min(1, (now - started) / 360), eased = 1 - Math.pow(1 - p, 3);
        state.scale = start.scale + (scale - start.scale) * eased; state.tx = start.tx + (tx - start.tx) * eased; state.ty = start.ty + (ty - start.ty) * eased; applyTransform();
        if (p < 1) state.animation = requestAnimationFrame(frame);
      };
      state.animation = requestAnimationFrame(frame);
    }
    function fit(animated = true) {
      const scale = Math.max(.02, Math.min(4, Math.min((viewport.clientWidth - 34) / viewBox.width, (viewport.clientHeight - 34) / viewBox.height)));
      moveTo(scale, (viewport.clientWidth - viewBox.width * scale) / 2, (viewport.clientHeight - viewBox.height * scale) / 2, animated);
    }
    function centerElements(items) {
      const vr = viewport.getBoundingClientRect();
      const rects = items.map((item) => item.getBoundingClientRect()).filter((rect) => rect.width && rect.height);
      if (!rects.length) return;
      const left = Math.min(...rects.map((rect) => rect.left - vr.left)), top = Math.min(...rects.map((rect) => rect.top - vr.top));
      const right = Math.max(...rects.map((rect) => rect.right - vr.left)), bottom = Math.max(...rects.map((rect) => rect.bottom - vr.top));
      const width = Math.max(1, right - left), height = Math.max(1, bottom - top);
      const scale = Math.max(.05, Math.min(2.5, state.scale * Math.min(1.5, (viewport.clientWidth - 72) / width, (viewport.clientHeight - 72) / height)));
      const graphX = ((left + right) / 2 - state.tx) / state.scale, graphY = ((top + bottom) / 2 - state.ty) / state.scale;
      moveTo(scale, viewport.clientWidth / 2 - graphX * scale, viewport.clientHeight / 2 - graphY * scale, true);
    }
    function pointTarget(x, y) {
      const candidates = document.elementsFromPoint(x, y).map(graphAncestor).filter(Boolean);
      return candidates.find((item) => item.classList.contains('node')) || candidates[0] || null;
    }
    function inEdgeRegion(edge, x, y) {
      if (!edge?.classList.contains('edge')) return false;
      const rect = state.hoverRect || edge.getBoundingClientRect(); state.hoverRect = rect;
      return x >= rect.left - 8 && x <= rect.right + 8 && y >= rect.top - 8 && y <= rect.bottom + 8;
    }
    function hitAt(x, y) { return inEdgeRegion(state.hover, x, y) ? state.hover : pointTarget(x, y); }
    function hideTooltip() { state.tooltipElement = null; state.tooltipEvent = null; tooltip.style.display = 'none'; }
    function drawTooltip() {
      state.tooltipFrame = 0; if (!state.tooltipElement || !state.tooltipEvent) return;
      const event = state.tooltipEvent; tooltip.textContent = titleOf(state.tooltipElement); tooltip.style.display = 'block';
      tooltip.style.left = `${Math.max(12, Math.min(event.clientX + 14, window.innerWidth - 372))}px`; tooltip.style.top = `${Math.max(12, Math.min(event.clientY + 14, window.innerHeight - 84))}px`;
    }
    function showTooltip(element, event) { state.tooltipElement = element; state.tooltipEvent = event; if (!state.tooltipFrame) state.tooltipFrame = requestAnimationFrame(drawTooltip); }
    function setHover(element, event) {
      if (state.hover !== element) {
        if (state.hover && !state.hover.classList.contains('is-pinned')) state.hover.classList.remove('is-hovered');
        state.hover = element; state.hoverRect = element?.classList.contains('edge') ? element.getBoundingClientRect() : null;
        if (element) element.classList.add('is-hovered');
      }
      if (element) showTooltip(element, event); else hideTooltip();
    }
    function clearPinned() { state.pinnedParts.forEach((item) => item.classList.remove('is-pinned')); state.pinnedParts.clear(); state.pinned = null; }
    function select(element, reverseEdge = false) {
      if (!element) return;
      if (state.pinned === element) { clearPinned(); selection.textContent = 'Hover an element for details. Drag to pan; scroll to zoom.'; return; }
      clearPinned(); state.pinned = element; state.pinnedParts = new Set(relatedTo(element)); state.pinnedParts.forEach((item) => item.classList.add('is-pinned')); selection.textContent = titleOf(element);
      if (element.classList.contains('edge')) { const ends = edgeEnds(element), target = ends[reverseEdge ? 0 : 1]; centerElements([...state.pinnedParts, ...(target ? [target] : [])]); } else centerElements([...state.pinnedParts]);
      state.hover = null; state.hoverRect = null;
    }
    function clearAll() { clearPinned(); setHover(null); elements.forEach((item) => item.classList.remove('is-dimmed', 'is-selected', 'is-related')); selection.textContent = 'Hover an element for details. Drag to pan; scroll to zoom.'; }

    viewport.addEventListener('pointermove', (event) => { if (!state.gesture?.dragging) setHover(hitAt(event.clientX, event.clientY), event); });
    viewport.addEventListener('pointerleave', () => { if (!state.gesture?.dragging) setHover(null); });
    viewport.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      state.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY }); state.pressTarget = pointTarget(event.clientX, event.clientY) || state.hover; viewport.setPointerCapture(event.pointerId);
      if (state.pointers.size >= 2) {
        const points = [...state.pointers.values()], mid = { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 }, vr = viewport.getBoundingClientRect();
        state.gesture = { type: 'pinch', distance: Math.max(1, Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y)), scale: state.scale, graphX: (mid.x - vr.left - state.tx) / state.scale, graphY: (mid.y - vr.top - state.ty) / state.scale, moved: false }; state.pressTarget = null;
      } else state.gesture = { type: 'pointer', x: event.clientX, y: event.clientY, tx: state.tx, ty: state.ty, moved: false };
    });
    viewport.addEventListener('pointermove', (event) => {
      if (!state.pointers.has(event.pointerId)) return;
      state.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (state.pointers.size >= 2 && state.gesture?.type === 'pinch') {
        const points = [...state.pointers.values()], mid = { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 }, vr = viewport.getBoundingClientRect();
        const scale = Math.max(.02, Math.min(4, state.gesture.scale * Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y) / state.gesture.distance));
        state.scale = scale; state.tx = mid.x - vr.left - state.gesture.graphX * scale; state.ty = mid.y - vr.top - state.gesture.graphY * scale; state.gesture.moved = true; viewport.classList.add('dragging'); scheduleTransform(); return;
      }
      if (state.gesture?.type !== 'pointer') return;
      const dx = event.clientX - state.gesture.x, dy = event.clientY - state.gesture.y;
      if (Math.hypot(dx, dy) > 4) { state.gesture.moved = true; state.gesture.dragging = true; viewport.classList.add('dragging'); }
      state.gesture.nextTx = state.gesture.tx + dx; state.gesture.nextTy = state.gesture.ty + dy;
      if (!state.gesture.frame) state.gesture.frame = requestAnimationFrame(() => { state.tx = state.gesture?.nextTx ?? state.tx; state.ty = state.gesture?.nextTy ?? state.ty; state.gesture.frame = 0; applyTransform(); });
    });
    function finish(event) {
      const gesture = state.gesture; state.pointers.delete(event.pointerId); if (state.pointers.size) return;
      if (gesture?.frame) cancelAnimationFrame(gesture.frame); if (gesture?.nextTx !== undefined) { state.tx = gesture.nextTx; state.ty = gesture.nextTy; applyTransform(); }
      if (gesture?.type === 'pointer' && !gesture.moved && state.pressTarget) { state.suppressClickUntil = performance.now() + 500; event.preventDefault(); select(pointTarget(event.clientX, event.clientY) || state.pressTarget, event.ctrlKey || event.metaKey); } else state.suppressClickUntil = performance.now() + 150;
      state.gesture = null; state.pressTarget = null; state.hoverRect = null; viewport.classList.remove('dragging');
    }
    viewport.addEventListener('pointerup', finish); viewport.addEventListener('pointercancel', finish);
    viewport.addEventListener('lostpointercapture', () => { if (!state.pointers.size) { state.gesture = null; state.pressTarget = null; viewport.classList.remove('dragging'); } });
    viewport.addEventListener('click', (event) => { if (state.suppressClickUntil > performance.now()) { event.preventDefault(); event.stopPropagation(); return; } if (event.target === viewport || event.target === canvas) clearAll(); });
    viewport.addEventListener('wheel', (event) => { event.preventDefault(); const rect = viewport.getBoundingClientRect(), x = event.clientX - rect.left, y = event.clientY - rect.top, scale = Math.max(.02, Math.min(4, state.scale * Math.pow(1.0018, -event.deltaY))); state.tx = x - (x - state.tx) * scale / state.scale; state.ty = y - (y - state.ty) * scale / state.scale; state.scale = scale; scheduleTransform(); }, { passive: false });
    viewport.addEventListener('keydown', (event) => { if (event.key !== 'Enter' && event.key !== ' ') return; const target = graphAncestor(event.target); if (target) { event.preventDefault(); select(target); } });
    document.getElementById('fit').addEventListener('click', () => { clearAll(); fit(); });
    document.getElementById('reset').addEventListener('click', () => { clearAll(); fit(false); });
    document.getElementById('zoom-in').addEventListener('click', () => { state.scale = Math.min(4, state.scale * 1.25); applyTransform(); });
    document.getElementById('zoom-out').addEventListener('click', () => { state.scale = Math.max(.02, state.scale / 1.25); applyTransform(); });
    search.addEventListener('input', () => { const query = search.value.trim().toLowerCase(); elements.forEach((item) => item.classList.toggle('is-dimmed', Boolean(query) && !titleOf(item).toLowerCase().includes(query))); });
    window.addEventListener('resize', () => { if (!state.pinned) fit(false); });
    fit(false);
  }
})();
