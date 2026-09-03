const STORAGE_KEY = 'radio-browser-plus-last-view';
const VOLUME_KEY = 'radio-browser-plus-volume';
const MUTED_KEY = 'radio-browser-plus-muted';
const VALID_VIEWS = new Set(['all', 'popular', 'favorites', 'countries', 'languages', 'tags', 'genres', 'search']);
const SEARCH_DEBOUNCE_MS = 300;
const STATION_REFRESH_INTERVAL_MS = 21600000; //every 6 hours - 6 * 60 * 60 * 1000
const MAX_CACHED_RESPONSES = 32;
const MAX_CONCURRENT_IMAGES = 4;
const VIRTUALIZATION_BUFFER = 200;
const SORT_WORKER_THRESHOLD = 100;
const responseCache = new Map();
const pendingRequests = new Map();
const appBase = document.body.dataset.basePath || '';
const metadataRefreshTracker = new Set();
let viewAbortController = null;
let imageLoadQueue = [];
let activeImageLoads = 0;
let sortWorker = null;
let favoritesLoaded = false;

function truncateName(name, maxLength = 50) {
  if (!name) return 'Unknown station';
  const value = String(name).trim();
  return value.length > maxLength ? value.substring(0, maxLength) + '...' : value;
}

function buildUrl(path) {
  return `${appBase}${path.startsWith('/') ? path : `/${path}`}`;
}

try {
  sortWorker = new Worker(buildUrl('/sort-worker.js'));
} catch {
  sortWorker = null;
}

function showLoadingSpinner() {
  const spinner = document.getElementById('loading-spinner');
  if (spinner) {
    spinner.classList.remove('hidden');
  }
}

function hideLoadingSpinner() {
  const spinner = document.getElementById('loading-spinner');
  if (spinner) {
    spinner.classList.add('hidden');
  }
}

function clearRetryTimer() {
  if (retryTimer) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
}

async function fetchJson(path, { cache = true, forceRefresh = false, signal, useViewSignal = true } = {}) {
  const url = buildUrl(path);
  if (cache && !forceRefresh && responseCache.has(url)) {
    return responseCache.get(url);
  }

  if (cache && !forceRefresh && pendingRequests.has(url)) {
    return pendingRequests.get(url);
  }

  const effectiveSignal = signal || (useViewSignal ? viewAbortController?.signal : undefined);
  const request = (async () => {
    const response = await fetch(url, { signal: effectiveSignal });
    if (!response.ok) {
      throw new Error(`Request failed: ${response.status}`);
    }

    const data = await response.json();
    if (cache) {
      if (responseCache.size >= MAX_CACHED_RESPONSES) {
        responseCache.delete(responseCache.keys().next().value);
      }
      responseCache.set(url, data);
    }
    return data;
  })();

  if (cache) {
    pendingRequests.set(url, request);
    request.then(
      () => pendingRequests.delete(url),
      () => pendingRequests.delete(url),
    );
  }

  return request;
}

async function processImageLoadQueue() {
  if (imageLoadQueue.length === 0 || activeImageLoads >= MAX_CONCURRENT_IMAGES) {
    return;
  }

  const { img, candidates, placeholderUrl, index } = imageLoadQueue.shift();
  activeImageLoads++;

  const tryNext = () => {
    if (index >= candidates.length) {
      img.onerror = null;
      img.onload = null;
      img.src = placeholderUrl || getLocalPlaceholderSvg();
      activeImageLoads--;
      processImageLoadQueue();
      return;
    }

    const nextSrc = candidates[index];
    img.onload = () => {
      img.onerror = null;
      img.onload = null;
      activeImageLoads--;
      processImageLoadQueue();
    };
    img.onerror = () => {
      img.onerror = null;
      img.onload = null;
      imageLoadQueue.unshift({ img, candidates, placeholderUrl, index: index + 1 });
      activeImageLoads--;
      processImageLoadQueue();
    };
    img.src = nextSrc;
  };

  if (!candidates.length) {
    img.onerror = null;
    img.onload = null;
    img.src = placeholderUrl || getLocalPlaceholderSvg();
    activeImageLoads--;
    processImageLoadQueue();
    return;
  }

  tryNext();
}

function syncPlaybackState() {
  isPlaying = !!activeAudio && !activeAudio.paused && !!currentStation;
  updatePlayStopButton();
}

function setupAudioRetry(station) {
  if (!activeAudio) return;

  activeAudio.onerror = () => {
    console.warn('Playback error detected, attempting retry...');
    isPlaying = false;
    updatePlayStopButton();
    attemptRetry(station);
  };

  activeAudio.onplay = () => {
    isPlaying = true;
    hideLoadingSpinner();
    updatePlayStopButton();
  };

  activeAudio.onpause = () => {
    isPlaying = false;
    hideLoadingSpinner();
    updatePlayStopButton();
  };

  activeAudio.onended = () => {
    isPlaying = false;
    hideLoadingSpinner();
    updatePlayStopButton();
  };

  activeAudio.onwaiting = () => {
    showLoadingSpinner();
    isPlaying = false;
    updatePlayStopButton();
  };

  activeAudio.onloadstart = () => {
    showLoadingSpinner();
  };

  activeAudio.oncanplay = () => {
    hideLoadingSpinner();
    syncPlaybackState();
  };
}

function attemptRetry(station) {
  if (!isPlaying || !station) {
    clearRetryTimer();
    hideLoadingSpinner();
    return;
  }

  if (state.retryAttempts >= state.maxRetries) {
    console.error('Max retries reached, giving up.');
    state.retryAttempts = 0;
    hideLoadingSpinner();
    return;
  }

  state.retryAttempts++;
  console.log(`Retry attempt ${state.retryAttempts} of ${state.maxRetries}...`);
  showLoadingSpinner();

  clearRetryTimer();
  retryTimer = setTimeout(() => {
    if (!isPlaying) return;
    
    const streamUrl = station.url_resolved || station.url;
    if (!streamUrl || !activeAudio) return;

    activeAudio.src = streamUrl;
    activeAudio.play().catch((err) => {
      console.error('Retry playback failed:', err);
      attemptRetry(station);
    });
  }, state.retryDelay);
}

const state = {
  view: 'all',
  searchQuery: '',
  favorites: {},
  filter: null,  // For filtered views like Countries:Japan
  muted: false,
  savedVolume: 1,
  retryAttempts: 0,
  maxRetries: 5,
  retryDelay: 3000, // 3 seconds
  currentUser: null,
};

const grid = document.getElementById('station-grid');
const viewTitle = document.getElementById('view-title');
const searchInput = document.getElementById('search');
const searchBtn = document.getElementById('search-btn');
const nowPlayingBar = document.getElementById('now-playing-bar');
const volumeSlider = document.getElementById('volume-slider');

let activeAudio = null;
let currentStation = null;
let isPlaying = false;
let retryTimer = null;
let renderedStations = [];
let stationImageObserver = null;
let stationById = new Map();
let searchTimer = null;
let searchController = null;

function setupStationImageObserver() {
  if (!grid) {
    return;
  }

  if (stationImageObserver) {
    stationImageObserver.disconnect();
  }

  stationImageObserver = new IntersectionObserver((entries) => {
    entries.forEach((entry) => {
      const img = entry.target;
      const stationId = img.dataset.stationImg;
      if (!stationId) {
        return;
      }

      const station = stationById.get(stationId);
      if (!station) {
        return;
      }

      if (entry.isIntersecting) {
        if (!img.dataset.loaded || img.dataset.loaded === 'false') {
          useStationImageWithFallback(img, station, img.dataset.placeholder || getLocalPlaceholderSvg());
          img.dataset.loaded = 'true';
        }
      }
    });
  }, {
    root: grid,
    rootMargin: '200px',
    threshold: 0.01,
  });

  grid.querySelectorAll('[data-station-img]').forEach((img) => {
    stationImageObserver.observe(img);
  });
}

function updateViewportSafeArea() {
  const root = document.documentElement;
  const viewport = window.visualViewport;
  const innerHeight = window.innerHeight;
  const clientHeight = document.documentElement.clientHeight || innerHeight;
  const viewportHeight = viewport ? viewport.height : clientHeight;
  const viewportOffsetTop = viewport ? viewport.offsetTop : 0;
  const visualInset = viewport ? Math.max(0, innerHeight - viewportHeight - viewportOffsetTop) : 0;
  const clientInset = Math.max(0, innerHeight - clientHeight);
  const cssSafeAreaInsetBottom = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue('--safe-area-bottom') || '0'
  ) || 0;

  const isMobile = /Mobi|Android|iPhone|iPad|iPod|mobile/i.test(navigator.userAgent)
    || window.matchMedia('(pointer: coarse)').matches
    || !!viewport;

  const safeBottom = isMobile ? Math.max(visualInset, clientInset, cssSafeAreaInsetBottom) : 0;
  const safeTop = viewport ? Math.max(0, viewportOffsetTop) : 0;

  root.style.setProperty('--viewport-height', `${clientHeight}px`);
  root.style.setProperty('--safe-top', `${safeTop}px`);
  root.style.setProperty('--safe-bottom', `${safeBottom}px`);
  root.style.setProperty('--mobile-bottom-gap', `${safeBottom}px`);

  if (isMobile) {
    document.body.style.height = `${clientHeight}px`;
    document.body.style.minHeight = `${clientHeight}px`;
    document.querySelector('.app-shell')?.style.setProperty('height', `${clientHeight}px`);
    document.querySelector('.app-shell')?.style.setProperty('max-height', `${clientHeight}px`);
    document.querySelector('.content')?.style.setProperty('height', `${clientHeight}px`);
    document.querySelector('.content')?.style.setProperty('max-height', `${clientHeight}px`);
  } else {
    document.body.style.height = '';
    document.body.style.minHeight = '';
    document.querySelector('.app-shell')?.style.removeProperty('height');
    document.querySelector('.app-shell')?.style.removeProperty('max-height');
    document.querySelector('.content')?.style.removeProperty('height');
    document.querySelector('.content')?.style.removeProperty('max-height');
  }
}

function getSavedVolume() {
  const saved = localStorage.getItem(VOLUME_KEY);
  return saved ? parseFloat(saved) : 1;
}

function setSavedVolume(volume) {
  state.savedVolume = volume;
  localStorage.setItem(VOLUME_KEY, volume.toString());
  if (activeAudio) {
    activeAudio.volume = volume;
  }
  updateVolumeDisplay();
}

function toggleMute() {
  state.muted = !state.muted;
  localStorage.setItem(MUTED_KEY, state.muted.toString());
  if (activeAudio) {
    activeAudio.volume = state.muted ? 0 : state.savedVolume;
  }
  updateVolumeDisplay();
}

function updateVolumeDisplay() {
  const volumeDisplay = document.getElementById('volume-display');
  const volumeIcon = document.getElementById('volume-icon');
  const safeVolume = Number.isFinite(state.savedVolume) ? Math.min(Math.max(state.savedVolume, 0), 1) : 0;
  const effectiveVolume = state.muted ? 0 : safeVolume;
  const percentage = Math.round(effectiveVolume * 100);

  if (volumeDisplay) {
    volumeDisplay.textContent = `${percentage}%`;
  }
  if (volumeIcon) {
    volumeIcon.textContent = state.muted ? '🔇' : effectiveVolume === 0 ? '🔈' : effectiveVolume < 0.5 ? '🔉' : '🔊';
  }
  if (volumeSlider) {
    volumeSlider.value = safeVolume.toFixed(2);
  }
}

function normalizeViewName(view) {
  if (!view) {
    return 'all';
  }

  const normalized = String(view).trim().toLowerCase();
  return VALID_VIEWS.has(normalized) ? normalized : 'all';
}

function syncNavSelection() {
  document.querySelectorAll('.nav').forEach((nav) => {
    nav.classList.toggle('active', nav.dataset.view === state.view);
  });
}

function updateURL() {
  if (state.view === 'search') {
    return;
  }

  const safeView = normalizeViewName(state.view);
  let hash = `#${safeView}`;
  if (state.filter && safeView !== 'all') {
    hash += `/${encodeURIComponent(state.filter)}`;
  }
  if (window.location.hash !== hash) {
    window.location.hash = hash;
  }
}

function parseURL() {
  const hash = window.location.hash.slice(1);
  if (!hash) {
    state.view = normalizeViewName(localStorage.getItem(STORAGE_KEY));
    state.filter = null;
    return;
  }

  const parts = hash.split('/');
  state.view = normalizeViewName(parts[0]);
  state.filter = parts[1] ? decodeURIComponent(parts[1]) : null;

  if (state.view === 'all') {
    state.filter = null;
  }
}

function restoreState() {
  const savedMuted = localStorage.getItem(MUTED_KEY);
  state.muted = savedMuted === 'true';
  state.savedVolume = getSavedVolume();
  parseURL();
}

function isLoggedIn() {
  return !!state.currentUser && !!state.currentUser.username;
}

function isAdminUser() {
  return isLoggedIn() && state.currentUser.role === 'admin';
}

function renderAuthControls() {
  const loginButton = document.getElementById('login-button');
  const logoutButton = document.getElementById('logout-button');
  const changePasswordButton = document.getElementById('change-password-button');
  const adminActions = document.getElementById('admin-actions');

  if (loginButton) {
    loginButton.classList.toggle('hidden', isLoggedIn());
  }

  if (logoutButton) {
    logoutButton.classList.toggle('hidden', !isLoggedIn());
    logoutButton.textContent = isLoggedIn() ? `Logout (${state.currentUser.username})` : 'Logout';
  }

  if (changePasswordButton) {
    changePasswordButton.classList.toggle('hidden', !isLoggedIn());
  }

  if (adminActions) {
    adminActions.classList.toggle('hidden', !isAdminUser());
  }
}

function showLoginModal() {
  const modal = document.getElementById('login-modal');
  if (modal) {
    modal.classList.remove('hidden');
    modal.setAttribute('aria-hidden', 'false');
  }
}

function hideLoginModal() {
  const modal = document.getElementById('login-modal');
  if (modal) {
    modal.classList.add('hidden');
    modal.setAttribute('aria-hidden', 'true');
  }
}

function showCreateUserModal() {
  const modal = document.getElementById('create-user-modal');
  if (modal) {
    modal.classList.remove('hidden');
    modal.setAttribute('aria-hidden', 'false');
  }
}

function hideCreateUserModal() {
  const modal = document.getElementById('create-user-modal');
  if (modal) {
    modal.classList.add('hidden');
    modal.setAttribute('aria-hidden', 'true');
  }
}

function showChangePasswordModal() {
  const modal = document.getElementById('change-password-modal');
  if (modal) {
    modal.classList.remove('hidden');
    modal.setAttribute('aria-hidden', 'false');
  }
}

function hideChangePasswordModal() {
  const modal = document.getElementById('change-password-modal');
  if (modal) {
    modal.classList.add('hidden');
    modal.setAttribute('aria-hidden', 'true');
  }
}

async function fetchCurrentUser() {
  const response = await fetch(buildUrl('/api/me'), { credentials: 'same-origin' });
  if (!response.ok) {
    state.currentUser = null;
    return null;
  }

  const user = await response.json();
  state.currentUser = user || null;
  renderAuthControls();
  return state.currentUser;
}

async function login(username, password) {
  const response = await fetch(buildUrl('/api/login'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify({ username, password }),
  });

  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || 'Login failed');
  }

  state.currentUser = payload || null;
  renderAuthControls();
  hideLoginModal();
  return payload;
}

async function logout() {
  try {
    await fetch(buildUrl('/api/logout'), {
      method: 'POST',
      credentials: 'same-origin',
    });
  } finally {
    state.currentUser = null;
    state.favorites = {};
    favoritesLoaded = false;
    renderAuthControls();
    showLoginModal();
  }
}

async function createUser(username, password, role = 'reader') {
  const response = await fetch(buildUrl('/api/users'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify({ username, password, role }),
  });

  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || 'User creation failed');
  }

  return payload;
}

async function changePassword(oldPassword, newPassword) {
  const response = await fetch(buildUrl('/api/change-password'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify({ old_password: oldPassword, new_password: newPassword }),
  });

  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || 'Password change failed');
  }

  return payload;
}

async function loadFavorites() {
  if (!isLoggedIn()) {
    state.favorites = {};
    favoritesLoaded = false;
    showLoginModal();
    return;
  }

  const response = await fetch(buildUrl('/api/favorites'), { credentials: 'same-origin' });
  if (response.status === 401 || response.status === 403) {
    state.favorites = {};
    favoritesLoaded = false;
    showLoginModal();
    return;
  }

  const json = await response.json();
  state.favorites = json.favorites || {};
  favoritesLoaded = true;
}

async function toggleFavorite(station) {
  if (!isLoggedIn()) {
    showLoginModal();
    return;
  }

  const stationId = getStationId(station);
  const streamUrl = station.url_resolved || station.url || null;
  const stationGenre = getStationGenre(station);
  const isRemoving = !!stationId && !!state.favorites[stationId];

  if (isRemoving) {
    const stationName = station?.name || 'this station';
    const confirmed = window.confirm(`Remove "${stationName}" from favorites?`);
    if (!confirmed) {
      return;
    }
  }

  const payload = {
    station_id: stationId,
    name: station.name,
    favicon: station.favicon || null,
    url: streamUrl,
    url_resolved: streamUrl,
    country: station.country || null,
    bitrate: station.bitrate ?? null,
    genre: stationGenre === 'Unknown genre' ? null : stationGenre,
    tags: Array.isArray(station.tags) ? station.tags : (station.tags ? [station.tags] : []),
  };

  if (!stationId) {
    return;
  }

  const response = await fetch(buildUrl('/api/favorites/toggle'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify(payload),
  });

  if (response.ok) {
    const result = await response.json();
    state.favorites = result?.favorites || {};
    favoritesLoaded = true;
    renderCurrentView();
  }
}

function getStationId(station) {
  return station?.stationuuid || station?.uuid || station?.id || null;
}

function normalizeBitrate(rawBitrate) {
  if (rawBitrate == null) {
    return null;
  }

  if (typeof rawBitrate === 'number') {
    if (!Number.isFinite(rawBitrate) || rawBitrate <= 0) {
      return null;
    }
    return `${rawBitrate} kbps`;
  }

  if (typeof rawBitrate === 'string') {
    const cleaned = rawBitrate.trim();
    if (!cleaned) {
      return null;
    }

    const compact = cleaned
      .replace(/kbit\s*\/\s*s/gi, 'kbps')
      .replace(/\s+kbps\s*/gi, ' kbps')
      .replace(/\s*(?:kbps|kbit\s+per\s+second|kilobits?\s+per\s+second)\s*$/gi, ' kbps')
      .trim();

    const withoutUnit = compact.replace(/\s*kbps\s*$/gi, '').trim();
    const numeric = Number.parseFloat(withoutUnit);
    if (!Number.isFinite(numeric) || numeric <= 0) {
      return compact && compact.toLowerCase() !== '0' ? compact : null;
    }

    return `${numeric} kbps`;
  }

  return null;
}

function formatBitrate(value) {
  const normalized = normalizeBitrate(value);
  if (!normalized) {
    return 'Stream';
  }

  return normalized;
}

function getFavoriteState(station) {
  const id = getStationId(station);
  return !!state.favorites[id];
}

function buildOptimizedImageUrl(url, targetSize = 128) {
  if (!url) {
    return url;
  }

  try {
    const parsed = new URL(url, window.location.href);
    const hostname = parsed.hostname || '';
    const pathname = parsed.pathname || '';

    if (hostname.includes('google.com') && pathname.includes('/s2/favicons')) {
      parsed.searchParams.set('sz', String(Math.max(32, Math.min(256, targetSize))));
      return parsed.toString();
    }

    if (parsed.searchParams.has('width') || parsed.searchParams.has('height')) {
      return url;
    }
  } catch {
    return url;
  }

  return url;
}

function buildImageFallbackCandidates(station) {
  const candidates = [];
  const directUrl = station?.favicon || station?.image || station?.logo || null;

  if (directUrl) {
    candidates.push(buildOptimizedImageUrl(directUrl, 128));
  }

  const homepage = station?.homepage || station?.website || station?.url || null;
  if (homepage) {
    try {
      const parsed = new URL(homepage);
      const domain = parsed.origin || parsed.href;
      if (domain) {
        const faviconUrl = `https://www.google.com/s2/favicons?sz=64&domain_url=${encodeURIComponent(domain)}`;
        candidates.push(buildOptimizedImageUrl(faviconUrl, 64));
      }
    } catch {
      // Ignore malformed URLs and continue with the direct favicon fallback.
    }
  }

  return [...new Set(candidates)];
}

function getLocalPlaceholderSvg() {
  return 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjQiIGhlaWdodD0iNjQiIHZpZXdCb3g9IjAgMCA2NCA2NCIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48Y2lyY2xlIGN4PSIzMiIgY3k9IjMyIiByPSIzMiIgZmlsbD0iIzJkMzU0OCIvPjxwYXRoIGQ9Ik0zMiAxNkMxNiAxNiAxNiAzMiAxNiA0OEgxNkMxNiAzMiAyNCAzMiAzMiAzMkM0MCAzMiA0OCAzMiA0OCA0OEg0OEM0OCAzMiA0OCAxNiAzMiAxNloiIGZpbGw9IiM5Y2EzYWYiLz48L3N2Zz4=';
}

function useStationImageWithFallback(img, station, placeholderUrl) {
  const candidates = buildImageFallbackCandidates(station);
  imageLoadQueue.push({ img, candidates, placeholderUrl, index: 0 });
  processImageLoadQueue();
}

function getStationGenre(station) {
  const genre = station?.tags || station?.tag || station?.genre || station?.language || null;
  if (!genre) {
    return 'Unknown genre';
  }

  if (Array.isArray(genre)) {
    return genre.filter(Boolean).slice(0, 2).join(', ') || 'Unknown genre';
  }

  return String(genre).trim() || 'Unknown genre';
}

async function refreshFavoriteMetadata(station) {
  const stationId = getStationId(station);
  if (!stationId || !state.favorites[stationId] || metadataRefreshTracker.has(stationId)) {
    return;
  }

  metadataRefreshTracker.add(stationId);

  try {
    const response = await fetch(buildUrl(`/api/station/${encodeURIComponent(stationId)}`));
    if (!response.ok) {
      return;
    }

    const liveStation = await response.json();
    if (!liveStation || typeof liveStation !== 'object') {
      return;
    }

    const liveGenre = getStationGenre(liveStation);
    if (liveGenre !== 'Unknown genre') {
      state.favorites[stationId] = {
        ...state.favorites[stationId],
        genre: liveGenre,
        tags: Array.isArray(liveStation.tags) ? liveStation.tags : (liveStation.tags ? [liveStation.tags] : state.favorites[stationId].tags || []),
        country: liveStation.country || state.favorites[stationId].country,
        bitrate: liveStation.bitrate ?? state.favorites[stationId].bitrate,
      };

      const updateResponse = await fetch(buildUrl('/api/favorites/update'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({
          station_id: stationId,
          name: liveStation.name || state.favorites[stationId].name,
          favicon: liveStation.favicon || state.favorites[stationId].favicon,
          url: liveStation.url_resolved || liveStation.url || state.favorites[stationId].url,
          url_resolved: liveStation.url_resolved || liveStation.url || state.favorites[stationId].url_resolved,
          country: liveStation.country || state.favorites[stationId].country,
          bitrate: liveStation.bitrate ?? state.favorites[stationId].bitrate,
          genre: liveGenre,
          tags: Array.isArray(liveStation.tags) ? liveStation.tags : (liveStation.tags ? [liveStation.tags] : state.favorites[stationId].tags || []),
        }),
      });

      if (updateResponse.ok) {
        const updatedFavorites = await updateResponse.json();
        state.favorites = updatedFavorites?.favorites || state.favorites;
      }
    }
  } catch {
    // Ignore metadata refresh failures; the station can still play without them.
  }
}

function updateNowPlayingStatus(station) {
  const statusBar = document.getElementById('now-playing-status-bar');
  if (!statusBar) {
    return;
  }

  const bitrateValue = Number.parseFloat(String(station?.bitrate || '').replace(/[^0-9.]/g, ''));
  const bitrateScore = Number.isFinite(bitrateValue)
    ? Math.min(1, Math.max(0.15, bitrateValue / 320))
    : 0.5;

  const healthScore = station?.lastcheckok === false ? 0.2 : station?.state === 'offline' ? 0.15 : station?.ssl_error ? 0.3 : 0.82;
  const httpsScore = station?.has_https === true ? 0.9 : station?.url?.startsWith('https') || station?.url_resolved?.startsWith('https') ? 0.8 : 0.45;
  const metadataScore = [station?.favicon, station?.homepage, station?.tags, station?.genre, station?.country]
    .filter(Boolean).length > 0 ? 0.8 : 0.45;
  const popularityScore = Number.isFinite(Number(station?.clickcount))
    ? Math.min(1, Math.max(0.2, Number(station.clickcount) / 10000))
    : 0.55;

  const score =
    bitrateScore * 0.35 +
    healthScore * 0.25 +
    httpsScore * 0.15 +
    metadataScore * 0.15 +
    popularityScore * 0.1;

  const safety = Math.max(0.12, Math.min(1, score));
  const height = Math.round(safety * 100);
  const hue = safety < 0.35 ? 0 : safety < 0.7 ? 35 : 120;
  const lightness = safety < 0.35 ? 58 : safety < 0.7 ? 52 : 50;

  const stateLabel = safety < 0.35 ? 'Poor' : safety < 0.7 ? 'Moderate' : 'Stable';
  const reasons = [];
  if (Number.isFinite(bitrateValue)) {
    reasons.push(`bitrate ${bitrateValue} kbps`);
  } else {
    reasons.push('bitrate unknown');
  }
  reasons.push(station?.lastcheckok === false ? 'stream check failed' : station?.state === 'offline' ? 'offline state' : 'recent check passed');
  reasons.push(station?.has_https === true || station?.url?.startsWith('https') || station?.url_resolved?.startsWith('https') ? 'HTTPS available' : 'HTTPS not confirmed');
  reasons.push(metadataScore > 0.7 ? 'station metadata is rich' : 'metadata is sparse');
  reasons.push(popularityScore > 0.6 ? 'popular stream' : 'low popularity signal');

  const tooltip = `${stateLabel} signal — ${reasons.join(', ')}.`;

  statusBar.title = tooltip;
  statusBar.setAttribute('aria-label', tooltip);
  statusBar.style.height = `${height}%`;
  statusBar.style.background = `linear-gradient(to top, hsl(${hue} 85% ${lightness}% / 0.95), hsl(${hue + 20} 90% ${Math.min(72, lightness + 10)}% / 0.9))`;
  statusBar.style.boxShadow = `0 0 10px hsl(${hue} 85% ${lightness}% / 0.55)`;
}

function playStation(station) {
  clearRetryTimer();
  state.retryAttempts = 0;
  showLoadingSpinner();

  if (activeAudio) {
    activeAudio.pause();
    activeAudio.src = '';
    activeAudio.onerror = null;
    activeAudio.onstalled = null;
    activeAudio.onsuspend = null;
  }

  const streamUrl = station.url_resolved || station.url;
  if (!streamUrl) {
    hideLoadingSpinner();
    return;
  }

  activeAudio = new Audio(streamUrl);
  activeAudio.volume = state.muted ? 0 : state.savedVolume;
  currentStation = station;
  nowPlayingBar.classList.remove('hidden');
  isPlaying = true;

  const bindAudioState = () => {
    syncPlaybackState();
  };
  activeAudio.addEventListener('play', bindAudioState);
  activeAudio.addEventListener('pause', bindAudioState);
  activeAudio.addEventListener('ended', bindAudioState);
  activeAudio.addEventListener('error', bindAudioState);

  const country = station.country || 'Unknown';
  const genre = getStationGenre(station);
  const bitrate = formatBitrate(station.bitrate);
  const placeholderUrl = getLocalPlaceholderSvg();

  const iconImg = document.getElementById('now-playing-icon');
  useStationImageWithFallback(iconImg, station, placeholderUrl);

  const nameEls = [
    document.getElementById('now-playing-name'),
    document.getElementById('now-playing-name-mobile'),
  ];
  const countryEls = [
    document.getElementById('now-playing-country'),
    document.getElementById('now-playing-country-mobile'),
  ];
  const genreEls = [
    document.getElementById('now-playing-genre'),
    document.getElementById('now-playing-genre-mobile'),
  ];
  const bitrateEls = [
    document.getElementById('now-playing-bitrate'),
    document.getElementById('now-playing-bitrate-mobile'),
  ];

  nameEls.filter(Boolean).forEach((el) => {
    el.textContent = truncateName(station.name);
  });
  countryEls.filter(Boolean).forEach((el) => {
    el.textContent = country;
  });
  genreEls.filter(Boolean).forEach((el) => {
    el.textContent = genre;
  });
  bitrateEls.filter(Boolean).forEach((el) => {
    el.textContent = bitrate;
  });

  document.getElementById('volume-slider').value = state.savedVolume;
  updateVolumeDisplay();
  updatePlayStopButton();
  updateFavoriteButton();
  updateNowPlayingStatus(station);
  updateNowPlayingLayout();

  if (state.favorites[getStationId(station)]) {
    refreshFavoriteMetadata(station);
  }

  setupAudioRetry(station);
  activeAudio.play().catch((err) => {
    console.error('Initial playback failed:', err);
    isPlaying = false;
    updatePlayStopButton();
    attemptRetry(station);
  });
}

function updateNowPlayingLayout() {
  const bar = document.getElementById('now-playing-bar');
  if (!bar) {
    return;
  }

  const mainMeta = bar.querySelector('.now-playing-main-row .now-playing-meta');
  const compactMeta = bar.querySelector('.now-playing-meta-row .now-playing-meta');
  const details = mainMeta ? mainMeta.querySelector('.now-playing-details') : null;
  const compactDetails = compactMeta ? compactMeta.querySelector('.now-playing-details') : null;

  if (!mainMeta || !details || !compactMeta || !compactDetails) {
    return;
  }

  const overflowDetected = (el) => !!(el && (el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1));
  const wasCompact = bar.classList.contains('compact');

  if (wasCompact) {
    bar.classList.remove('compact');
  }

  const needsCompact = overflowDetected(details) || overflowDetected(mainMeta) || overflowDetected(compactDetails) || overflowDetected(compactMeta);

  if (needsCompact) {
    bar.classList.add('compact');
  } else {
    bar.classList.remove('compact');
  }
}

function pauseStation() {
  clearRetryTimer();
  state.retryAttempts = 0;
  hideLoadingSpinner();
  if (activeAudio) {
    activeAudio.pause();
    isPlaying = false;
    updatePlayStopButton();
  }
}

function resumeStation() {
  if (activeAudio && currentStation) {
    activeAudio.play().then(() => {
      isPlaying = true;
      updatePlayStopButton();
    }).catch((err) => {
      console.error('Resume playback failed:', err);
      isPlaying = false;
      updatePlayStopButton();
    });
  }
}

function updatePlayStopButton() {
  const btn = document.getElementById('play-stop-btn');
  if (!btn) return;

  if (isPlaying) {
    btn.textContent = '■';
    btn.setAttribute('aria-label', 'Stop playback');
    btn.title = 'Stop';
    btn.classList.add('is-playing');
  } else if (currentStation) {
    btn.textContent = '▶';
    btn.setAttribute('aria-label', 'Resume playback');
    btn.title = 'Play';
    btn.classList.remove('is-playing');
  } else {
    btn.textContent = '▶';
    btn.setAttribute('aria-label', 'Play');
    btn.title = 'Play';
    btn.classList.remove('is-playing');
  }
}

function updateFavoriteButton() {
  if (!currentStation) return;
  const btn = document.getElementById('favorite-current');
  const id = getStationId(currentStation);
  const isFavorited = !!state.favorites[id];
  btn.textContent = isFavorited ? '★' : '☆';
  btn.classList.toggle('is-favorited', isFavorited);
  btn.setAttribute('aria-label', isFavorited ? 'Remove from favorites' : 'Add to favorites');
  btn.title = isFavorited ? 'Remove from favorites' : 'Add to favorites';
}

async function toggleCurrentFavorite() {
  if (!currentStation) return;
  await toggleFavorite(currentStation);
  updateFavoriteButton();
}

function createStationCard(station, template) {
  const id = getStationId(station);
  const favorited = getFavoriteState(station);
  const country = station.country || 'Unknown';
  const bitrate = formatBitrate(station.bitrate);
  const truncatedName = truncateName(station.name);
  const safeName = truncatedName.replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const placeholderUrl = getLocalPlaceholderSvg();

  const clone = template.content.cloneNode(true);
  const article = clone.querySelector('article');
  article.dataset.station = id;
  article.classList.toggle('favorite', favorited);

  const favoriteBtn = clone.querySelector('[data-favorite]');
  favoriteBtn.dataset.favorite = id;
  favoriteBtn.textContent = favorited ? '★' : '☆';

  const img = clone.querySelector('img');
  img.src = placeholderUrl;
  img.alt = safeName;
  img.dataset.stationImg = id;
  img.dataset.placeholder = placeholderUrl;

  const nameDiv = clone.querySelector('.station-name');
  nameDiv.textContent = safeName;

  const metaDiv = clone.querySelector('.station-meta');
  metaDiv.textContent = `${country} · ${bitrate}`;

  return clone;
}

function renderStations(stations) {
  const scrollTop = grid?.scrollTop || 0;
  renderedStations = stations || [];
  stationById = new Map(renderedStations.map((station) => [getStationId(station), station]));

  if (!stations.length) {
    grid.innerHTML = '<div class="station-card"><h3>No stations found</h3></div>';
    grid.scrollTop = scrollTop;
    return;
  }

  const stationTemplate = document.getElementById('station-card-template');
  const useTemplate = stationTemplate && stationTemplate.content;

  const fragment = document.createDocumentFragment();
  const maxVirtualized = 1000;
  const shouldVirtualize = stations.length > maxVirtualized;

  stations.slice(0, shouldVirtualize ? maxVirtualized : stations.length).forEach((station) => {
    const cardNode = useTemplate ? createStationCard(station, stationTemplate) : createLegacyStationCard(station);
    fragment.appendChild(cardNode);
  });
  grid.innerHTML = '';
  grid.appendChild(fragment);
  grid.scrollTop = scrollTop;

  setupStationImageObserver();
  metadataRefreshTracker.clear();
}

function createLegacyStationCard(station) {
  const id = getStationId(station);
  const favorited = getFavoriteState(station);
  const country = station.country || 'Unknown';
  const bitrate = formatBitrate(station.bitrate);
  const truncatedName = truncateName(station.name);
  const placeholderUrl = getLocalPlaceholderSvg();

  const article = document.createElement('article');
  article.className = `station-card${favorited ? ' favorite' : ''}`;
  article.dataset.station = id;

  const favoriteButton = document.createElement('button');
  favoriteButton.className = 'favorite-button';
  favoriteButton.dataset.favorite = id;
  favoriteButton.setAttribute('aria-label', 'Toggle favorite');
  favoriteButton.textContent = favorited ? '★' : '☆';

  const art = document.createElement('div');
  art.className = 'card-art';
  const image = document.createElement('img');
  image.src = placeholderUrl;
  image.alt = truncatedName;
  image.dataset.stationImg = id;
  image.dataset.placeholder = placeholderUrl;
  image.loading = 'lazy';
  image.decoding = 'async';
  art.appendChild(image);

  const details = document.createElement('div');
  const name = document.createElement('div');
  name.className = 'station-name';
  name.textContent = truncatedName;
  const meta = document.createElement('div');
  meta.className = 'station-meta';
  meta.textContent = `${country} · ${bitrate}`;
  details.append(name, meta);

  article.append(favoriteButton, art, details);
  return article;
}

grid.addEventListener('click', async (event) => {
  const favoriteButton = event.target.closest('[data-favorite]');
  if (favoriteButton) {
    event.stopPropagation();
    const station = stationById.get(favoriteButton.dataset.favorite);
    if (station) {
      await toggleFavorite(station);
    }
    return;
  }

  const stationCard = event.target.closest('[data-station]');
  if (stationCard) {
    const station = stationById.get(stationCard.dataset.station);
    if (station) {
      playStation(station);
    }
    return;
  }

  const collectionCard = event.target.closest('[data-collection]');
  if (!collectionCard) {
    return;
  }

  const kind = collectionCard.dataset.collection || 'genres';
  const key = decodeURIComponent(collectionCard.dataset.key || '');
  const filterKey = decodeURIComponent(collectionCard.dataset.filterKey || key);
  let path = '';
  if (kind === 'countries') {
    path = `/api/stations?country=${encodeURIComponent(filterKey)}`;
  } else if (kind === 'languages') {
    path = `/api/stations?language=${encodeURIComponent(filterKey)}`;
  } else if (kind === 'tags' || kind === 'genres') {
    path = `/api/stations?tag=${encodeURIComponent(filterKey.toLowerCase())}`;
  }

  if (!path) {
    return;
  }

  resetStationListScroll();
  const stations = await fetchJson(path);
  renderStations(kind === 'genres' ? sortByNameAsc(stations) : stations);
  const displayName = kind === 'genres' ? 'Genres' : kind.charAt(0).toUpperCase() + kind.slice(1);
  viewTitle.textContent = `${displayName}: ${key}`;
  state.view = kind;
  state.filter = key;
  updateURL();
});

async function fetchAllStations() {
  const stations = await fetchJson('/api/stations');
  return sortByNameAscWorker(stations);
}

async function refreshAllStations({ render = true } = {}) {
  try {
    await fetchJson('/api/stations', { forceRefresh: true, useViewSignal: false });
    if (render && state.view === 'all' && !state.filter) {
      await renderCurrentView();
    }
  } catch (error) {
    console.error('Station refresh failed:', error);
  }
}

function startStationRefreshTimer() {
  window.setInterval(() => {
    refreshAllStations();
    refreshPopularStations();
    refreshSupportingViews();
  }, STATION_REFRESH_INTERVAL_MS);
}

async function fetchPopular() {
  return fetchJson('/api/popular');
}

async function refreshPopularStations({ render = true } = {}) {
  try {
    await fetchJson('/api/popular', { forceRefresh: true, useViewSignal: false });
    if (render && state.view === 'popular' && !state.filter) {
      await renderCurrentView();
    }
  } catch (error) {
    console.error('Popular station refresh failed:', error);
  }
}

async function refreshSupportingViews({ render = true } = {}) {
  const requests = [
    fetchJson('/api/countries', { forceRefresh: true, useViewSignal: false }),
    fetchJson('/api/languages', { forceRefresh: true, useViewSignal: false }),
    fetchJson('/api/tags', { forceRefresh: true, useViewSignal: false }),
  ];

  if (isLoggedIn()) {
    requests.push(loadFavorites());
  }

  try {
    await Promise.all(requests);
    if (render && !state.filter && ['countries', 'languages', 'tags', 'genres', 'favorites'].includes(state.view)) {
      await renderCurrentView();
    }
  } catch (error) {
    console.error('Supporting view refresh failed:', error);
  }
}

function sortByNameAsc(items) {
  return [...items].sort((a, b) => {
    const left = String(a.name || a.country || a.language || a.label || '').trim().toLowerCase();
    const right = String(b.name || b.country || b.language || b.label || '').trim().toLowerCase();
    return left.localeCompare(right);
  });
}

async function sortByNameAscWorker(items) {
  if (!sortWorker || items.length < SORT_WORKER_THRESHOLD) {
    return sortByNameAsc(items);
  }

  return new Promise((resolve) => {
    const messageHandler = (event) => {
      if (event.data.type === 'sortByName') {
        sortWorker.removeEventListener('message', messageHandler);
        resolve(event.data.result);
      }
    };
    sortWorker.addEventListener('message', messageHandler);
    sortWorker.postMessage({ type: 'sortByName', items });
  });
}

async function fetchFavorites() {
  if (!favoritesLoaded) {
    await loadFavorites();
  }

  const favorites = Object.entries(state.favorites);
  const sorted = favorites.map(([stationId, item]) => ({
    stationuuid: stationId,
    name: item.name,
    favicon: item.favicon,
    country: item.country || 'Favorite',
    bitrate: normalizeBitrate(item.bitrate) || 'Stream',
    genre: item.genre || null,
    tags: item.tags || (item.genre ? [item.genre] : []),
    url: item.url_resolved || item.url || '',
    url_resolved: item.url_resolved || item.url || '',
  }));

  return sortByNameAscWorker(sorted);
}

async function fetchCountries() {
  const countries = await fetchJson('/api/countries');
  return sortByNameAscWorker(countries);
}

async function fetchLanguages() {
  const languages = await fetchJson('/api/languages');
  return sortByNameAscWorker(languages);
}

async function fetchTags() {
  const tags = await fetchJson('/api/tags');
  return sortByNameAscWorker(tags);
}

function renderCollection(list, kind) {
  if (!list.length) {
    grid.innerHTML = '<div class="station-card"><h3>No items found</h3></div>';
    return;
  }

  const collectionKind = kind === 'genres' ? 'genres' : kind;
  const collectionTemplate = document.getElementById('collection-card-template');
  const useTemplate = collectionTemplate && collectionTemplate.content;

  const fragment = document.createDocumentFragment();

  list.forEach((item) => {
    if (!useTemplate) {
      return;
    }
    const label = item.name || item.country || item.language || item.name || 'Unknown';
    const count = item.stationcount ?? item.count ?? '';
    const filterValue = item.name || label;
    let iconUrl = item.favicon || getLocalPlaceholderSvg();

    if (kind === 'countries' && item.iso_3166_1) {
      iconUrl = `/flags/${item.iso_3166_1.toLowerCase()}.svg`;
    }

    if (useTemplate) {
      const clone = collectionTemplate.content.cloneNode(true);
      const article = clone.querySelector('article');
      article.dataset.collection = collectionKind;
      article.dataset.key = encodeURIComponent(label);
      article.dataset.filterKey = encodeURIComponent(filterValue);

      const img = clone.querySelector('img');
      img.src = iconUrl;
      img.alt = label;

      const nameDiv = clone.querySelector('.station-name');
      nameDiv.textContent = label;

      const metaDiv = clone.querySelector('.station-meta');
      metaDiv.textContent = count ? `${count} stations` : collectionKind;

      fragment.appendChild(clone);
    } else {
      const template = document.createElement('template');
      template.innerHTML = `
        <article class="station-card" data-collection="${collectionKind}" data-key="${encodeURIComponent(label)}" data-filter-key="${encodeURIComponent(filterValue)}">
          <div class="card-art">
            <img src="${iconUrl}" alt="${label}" onerror="this.src='data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjQiIGhlaWdodD0iNjQiIHZpZXdCb3g9IjAgMCA2NCA2NCIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cmVjdCB3aWR0aD0iNjQiIGhlaWdodD0iNjQiIGZpbGw9IiMyZDM1NDgiLz48Y2lyY2xlIGN4PSIzMiIgY3k9IjMyIiByPSIyNCIgZmlsbD0ibm9uZSIgc3Ryb2tlPSIjOWNhM2FmIiBzdHJva2Utd2lkdGg9IjIiLz48L3N2Zz4='" />
          </div>
          <div>
            <div class="station-name">${label}</div>
            <div class="station-meta">${count ? `${count} stations` : collectionKind}</div>
          </div>
        </article>
      `;
      fragment.appendChild(template.content.cloneNode(true));
    }
  });

  grid.innerHTML = '';
  grid.appendChild(fragment);
}

function resetStationListScroll() {
  if (grid) {
    grid.scrollTop = 0;
  }
}

async function renderCurrentView() {
  if (viewAbortController) {
    viewAbortController.abort();
  }
  viewAbortController = new AbortController();

  const view = state.view;

  if (view === 'search') {
    const query = state.searchQuery.trim();
    if (!query) {
      state.view = 'popular';
      await renderCurrentView();
      return;
    }

    try {
      const stations = await fetchJson(`/api/search?q=${encodeURIComponent(query)}`);
      if (state.view === 'search' && state.searchQuery === query) {
        renderStations(stations);
        viewTitle.textContent = `Search: ${query}`;
      }
    } catch (error) {
      if (error.name !== 'AbortError') {
        console.error('Search failed:', error);
      }
    }
    return;
  }

  if (state.filter) {
    let stations = [];
    try {
      if (view === 'countries') {
        stations = await fetchJson(`/api/stations?country=${encodeURIComponent(state.filter)}`);
      } else if (view === 'languages') {
        stations = await fetchJson(`/api/stations?language=${encodeURIComponent(state.filter)}`);
      } else if (view === 'tags' || view === 'genres') {
        stations = await fetchJson(`/api/stations?tag=${encodeURIComponent(state.filter.toLowerCase())}`);
      }
      if (state.view === view && state.filter) {
        if (view === 'genres') {
          stations = await sortByNameAscWorker(stations);
        }
        const displayView = view === 'genres' ? 'Genres' : view.charAt(0).toUpperCase() + view.slice(1);
        renderStations(stations);
        viewTitle.textContent = `${displayView}: ${state.filter}`;
      }
    } catch (error) {
      if (error.name !== 'AbortError') {
        console.error('Filtered view failed:', error);
      }
    }
    return;
  }

  try {
    if (view === 'all') {
      const stations = await fetchAllStations();
      if (state.view === view) {
        renderStations(stations);
        viewTitle.textContent = 'All stations';
      }
      return;
    }
    if (view === 'countries') {
      const countries = await fetchCountries();
      if (state.view === view) {
        renderCollection(countries, 'countries');
        viewTitle.textContent = 'Countries';
      }
      return;
    }
    if (view === 'languages') {
      const languages = await fetchLanguages();
      if (state.view === view) {
        renderCollection(languages, 'languages');
        viewTitle.textContent = 'Languages';
      }
      return;
    }
    if (view === 'tags' || view === 'genres') {
      const tags = await fetchTags();
      if (state.view === view) {
        renderCollection(tags, view === 'genres' ? 'genres' : 'tags');
        viewTitle.textContent = 'Genres';
      }
      return;
    }

    viewTitle.textContent = view === 'popular' ? 'Popular' : view.charAt(0).toUpperCase() + view.slice(1);

    let stations = [];
    if (view === 'popular') {
      stations = await fetchPopular();
    } else if (view === 'favorites') {
      stations = await fetchFavorites();
    }

    if (state.view === view) {
      renderStations(stations);
    }
  } catch (error) {
    if (error.name !== 'AbortError') {
      console.error('View rendering failed:', error);
    }
  }
}

function persistView(view) {
  if (!VALID_VIEWS.has(view)) {
    return;
  }
  localStorage.setItem(STORAGE_KEY, view);
}

for (const button of document.querySelectorAll('.nav')) {
  button.addEventListener('click', async () => {
    if (viewAbortController) {
      viewAbortController.abort();
    }
    state.view = normalizeViewName(button.dataset.view);
    state.filter = null;
    resetStationListScroll();
    persistView(state.view);
    syncNavSelection();
    updateURL();
    await renderCurrentView();
  });
}

window.addEventListener('hashchange', async () => {
  parseURL();
  resetStationListScroll();
  syncNavSelection();
  await renderCurrentView();
});

async function applySearch(query) {
  const trimmed = query.trim();
  state.searchQuery = trimmed;

  if (!trimmed) {
    state.view = 'popular';
    viewTitle.textContent = 'Popular';
    await renderCurrentView();
    return;
  }

  state.view = 'search';
  clearTimeout(searchTimer);
  searchController?.abort();
  searchController = new AbortController();
  searchTimer = setTimeout(async () => {
    try {
      const stations = await fetchJson(
        `/api/search?q=${encodeURIComponent(trimmed)}`,
        { signal: searchController.signal },
      );
      if (state.view === 'search' && state.searchQuery === trimmed) {
        renderStations(stations);
        viewTitle.textContent = `Search: ${trimmed}`;
      }
    } catch (error) {
      if (error.name !== 'AbortError') {
        console.error('Search failed:', error);
      }
    }
  }, SEARCH_DEBOUNCE_MS);
}

searchInput.addEventListener('input', () => {
  applySearch(searchInput.value);
});

searchBtn.addEventListener('click', () => {
  clearTimeout(searchTimer);
  applySearch(searchInput.value);
});

const menuToggle = document.getElementById('menu-toggle');
const sidebar = document.querySelector('.sidebar');

function closeMobileMenu() {
  if (sidebar && window.innerWidth <= 768) {
    sidebar.classList.remove('open');
  }
}

menuToggle.addEventListener('click', () => {
  if (sidebar) {
    sidebar.classList.toggle('open');
  }
});

// Close menu when clicking on nav items
document.querySelectorAll('.nav').forEach((nav) => {
  nav.addEventListener('click', closeMobileMenu);
});

// Close menu when clicking outside
document.addEventListener('click', (e) => {
  if (sidebar && menuToggle && sidebar.classList.contains('open')) {
    if (!sidebar.contains(e.target) && !menuToggle.contains(e.target)) {
      closeMobileMenu();
    }
  }
});

const favoriteCurrentBtn = document.getElementById('favorite-current');
favoriteCurrentBtn.addEventListener('click', async () => {
  await toggleCurrentFavorite();
});

const loginButton = document.getElementById('login-button');
if (loginButton) {
  loginButton.addEventListener('click', () => {
    showLoginModal();
  });
}

const logoutButton = document.getElementById('logout-button');
if (logoutButton) {
  logoutButton.addEventListener('click', async () => {
    await logout();
  });
}

const createUserButton = document.getElementById('create-user-button');
if (createUserButton) {
  createUserButton.addEventListener('click', () => {
    if (!isAdminUser()) {
      return;
    }
    showCreateUserModal();
  });
}

const changePasswordButton = document.getElementById('change-password-button');
if (changePasswordButton) {
  changePasswordButton.addEventListener('click', () => {
    if (!isLoggedIn()) {
      return;
    }
    showChangePasswordModal();
  });
}

const loginForm = document.getElementById('login-form');
if (loginForm) {
  loginForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    const username = document.getElementById('login-username').value.trim();
    const password = document.getElementById('login-password').value;
    if (!username || !password) {
      return;
    }

    try {
      await login(username, password);
      await loadFavorites();
      await renderCurrentView();
    } catch (error) {
      window.alert(error.message || 'Login failed');
    }
  });
}

const createUserForm = document.getElementById('create-user-form');
if (createUserForm) {
  createUserForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isAdminUser()) {
      return;
    }

    const username = document.getElementById('create-user-username').value.trim();
    const password = document.getElementById('create-user-password').value;
    const role = document.getElementById('create-user-role').value;
    if (!username || !password) {
      return;
    }

    try {
      await createUser(username, password, role);
      createUserForm.reset();
      hideCreateUserModal();
      window.alert(`User "${username}" created successfully.`);
    } catch (error) {
      window.alert(error.message || 'User creation failed');
    }
  });
}

const changePasswordForm = document.getElementById('change-password-form');
if (changePasswordForm) {
  changePasswordForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isLoggedIn()) {
      return;
    }

    const oldPassword = document.getElementById('change-current-password').value;
    const newPassword = document.getElementById('change-new-password').value;
    if (!oldPassword || !newPassword) {
      return;
    }

    try {
      await changePassword(oldPassword, newPassword);
      changePasswordForm.reset();
      hideChangePasswordModal();
      window.alert('Password updated successfully.');
    } catch (error) {
      window.alert(error.message || 'Password change failed');
    }
  });
}

for (const closeButton of document.querySelectorAll('[data-close]')) {
  closeButton.addEventListener('click', () => {
    const target = closeButton.dataset.close;
    if (target === 'login-modal') {
      hideLoginModal();
    } else if (target === 'create-user-modal') {
      hideCreateUserModal();
    } else if (target === 'change-password-modal') {
      hideChangePasswordModal();
    }
  });
}

const authModalBackdrop = document.querySelectorAll('.auth-modal-backdrop');
authModalBackdrop.forEach((backdrop) => {
  backdrop.addEventListener('click', () => {
    const modal = backdrop.parentElement;
    if (modal?.id === 'login-modal') {
      hideLoginModal();
    } else if (modal?.id === 'create-user-modal') {
      hideCreateUserModal();
    } else if (modal?.id === 'change-password-modal') {
      hideChangePasswordModal();
    }
  });
});

const playStopBtn = document.getElementById('play-stop-btn');
playStopBtn.addEventListener('click', () => {
  if (isPlaying) {
    pauseStation();
  } else if (currentStation) {
    resumeStation();
  }
});

volumeSlider.addEventListener('input', (e) => {
  const volume = parseFloat(e.target.value);
  state.muted = false;
  setSavedVolume(volume);
});

const volumeIcon = document.getElementById('volume-icon');
volumeIcon.addEventListener('click', () => {
  toggleMute();
});

window.addEventListener('resize', () => {
  updateViewportSafeArea();
  requestAnimationFrame(updateNowPlayingLayout);
});

if (window.visualViewport) {
  window.visualViewport.addEventListener('resize', () => {
    updateViewportSafeArea();
    requestAnimationFrame(updateNowPlayingLayout);
  });
  window.visualViewport.addEventListener('scroll', () => {
    updateViewportSafeArea();
    requestAnimationFrame(updateNowPlayingLayout);
  });
}

// Keyboard shortcuts - only active when no text input is focused
function isTypingFieldFocused() {
  const activeElement = document.activeElement;
  return activeElement && (activeElement.tagName === 'INPUT' || activeElement.tagName === 'TEXTAREA');
}

document.addEventListener('keydown', (e) => {
  // Don't handle keyboard shortcuts if user is typing in an input field
  if (isTypingFieldFocused()) {
    return;
  }

  const key = e.key.toLowerCase();

  // M - toggle mute
  if (key === 'm') {
    e.preventDefault();
    toggleMute();
  }
  // Spacebar - toggle play/stop
  else if (e.code === 'Space') {
    e.preventDefault();
    if (isPlaying) {
      pauseStation();
    } else if (currentStation) {
      resumeStation();
    }
  }
  // Left arrow - volume down (-5%)
  else if (e.code === 'ArrowLeft') {
    e.preventDefault();
    const currentVolume = parseFloat(volumeSlider.value);
    const newVolume = Math.max(0, currentVolume - 0.05); // 5% decrease
    state.muted = false;
    setSavedVolume(newVolume);
  }
  // Right arrow - volume up (+5%)
  else if (e.code === 'ArrowRight') {
    e.preventDefault();
    const currentVolume = parseFloat(volumeSlider.value);
    const newVolume = Math.min(1, currentVolume + 0.05); // 5% increase
    state.muted = false;
    setSavedVolume(newVolume);
  }
});

(async function init() {
  updateViewportSafeArea();

  // Initialize state from URL and storage
  restoreState();

  // Set initial volume slider
  volumeSlider.value = state.savedVolume;
  updateVolumeDisplay();
  renderAuthControls();
  syncNavSelection();

  const allStationsPromise = refreshAllStations({ render: false });
  const popularStationsPromise = refreshPopularStations({ render: false });

  try {
    await fetchCurrentUser();
  } catch {
    state.currentUser = null;
    renderAuthControls();
  }

  const supportingViewsPromise = refreshSupportingViews({ render: false });

  if (state.view === 'all') {
    await allStationsPromise;
  } else if (state.view === 'popular') {
    await popularStationsPromise;
  } else {
    await supportingViewsPromise;
  }
  await renderCurrentView();
  startStationRefreshTimer();

  await Promise.all([popularStationsPromise, supportingViewsPromise]);
})();
