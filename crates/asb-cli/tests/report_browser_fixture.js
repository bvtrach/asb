// Optional real-browser qualification, appended to an owned generated report.
try {
  const chart=document.querySelector('[data-graph="series-profile.latency_ms.svg"]');
  const points=chart.querySelectorAll('.series-point');
  if(points.length!==2||points[1].dataset.timestamp!=='200')throw new Error('misaligned generated observations');
  const line=chart.querySelectorAll('.series')[1];
  const bounds=points[1].getBoundingClientRect();
  line.dispatchEvent(new PointerEvent('pointerenter',{clientX:bounds.x+bounds.width/2,clientY:bounds.y+bounds.height/2}));
  const tooltip=chart.querySelector('.chart-tooltip');
  if(!tooltip.textContent.includes('Nearest displayed observed sample: 000-0: 3 @ 200 ms'))throw new Error('incorrect observed hover: '+tooltip.textContent);
  const svg=chart.querySelector('svg'),original=svg.getAttribute('viewBox');
  svg.dispatchEvent(new WheelEvent('wheel',{deltaY:-100,clientX:bounds.x,clientY:bounds.y,cancelable:true}));
  if(svg.getAttribute('viewBox')===original)throw new Error('zoom did not update');
  chart.querySelector('.chart-reset').click();
  if(svg.getAttribute('viewBox')!==original)throw new Error('reset did not restore axes');
  chart.querySelector('.chart-expand').click();
  if(!chart.classList.contains('expanded'))throw new Error('modal did not open');
  chart.querySelector('.chart-close').click();
  const filter=document.querySelector('input[data-group="agent"]');
  filter.checked=false;filter.dispatchEvent(new Event('change'));
  if(!document.querySelector('#results tr').hidden||chart.querySelector('svg [data-benchmark]').style.display!=='none')throw new Error('filter did not update table and generated graph');
  filter.checked=true;filter.dispatchEvent(new Event('change'));
  if(document.querySelector('#results tr').hidden||chart.querySelector('svg [data-benchmark]').style.display!=='')throw new Error('selection restoration failed');
  document.documentElement.setAttribute('data-asb-browser-test','passed');
} catch(error) {
  document.documentElement.setAttribute('data-asb-browser-test','FAILED: '+error.message);
}
