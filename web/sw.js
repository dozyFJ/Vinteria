// The build script stamps this with a hash of the complete application.
const cacheName = 'winteria-studio-BUILD_ID';
const files = [
  './', './index.html', './style.css', './boot.js', './manifest.webmanifest',
  './assets/icon-192.png', './assets/icon-512.png',
  './pkg/winteria.js', './pkg/winteria_bg.wasm'
];
self.addEventListener('install', event => {
  event.waitUntil(caches.open(cacheName).then(cache =>
    cache.addAll(files.map(path => new Request(path, { cache: 'reload' })))
  ));
});
self.addEventListener('activate', event => {
  event.waitUntil(caches.keys().then(keys => Promise.all(keys
    .filter(key => key.startsWith('winteria-studio-') && key !== cacheName)
    .map(key => caches.delete(key)))));
});
self.addEventListener('fetch', event => {
  if (event.request.method !== 'GET' || new URL(event.request.url).origin !== self.location.origin) return;
  event.respondWith(caches.open(cacheName).then(async cache => {
    if (event.request.mode === 'navigate') {
      return await cache.match('./index.html') || fetch(event.request);
    }
    return await cache.match(event.request) || fetch(event.request);
  }));
});
