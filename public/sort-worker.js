// Web Worker for sorting large station lists off the main thread
self.onmessage = function(event) {
  const { type, items } = event.data;

  if (type === 'sortByName') {
    const sorted = [...items].sort((a, b) => {
      const left = String(a.name || a.country || a.language || a.label || '').trim().toLowerCase();
      const right = String(b.name || b.country || b.language || b.label || '').trim().toLowerCase();
      return left.localeCompare(right);
    });
    self.postMessage({ type: 'sortByName', result: sorted });
  }

  if (type === 'sortByPopularity') {
    const sorted = [...items].sort((a, b) => {
      const clickA = Number(a.clickcount || 0);
      const clickB = Number(b.clickcount || 0);
      return clickB - clickA;
    });
    self.postMessage({ type: 'sortByPopularity', result: sorted });
  }
};
