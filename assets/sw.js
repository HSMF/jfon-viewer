var cacheName = "jfon-viewer";
var filesToCache = [
  "./",
  "./index.html",
  "./analyzer-d17e2077c18fa0c.js",
  "./analyzer-d17e2077c18fa0c_bg.wasm",
];

/* Start the service worker and cache all of the app's content */
self.addEventListener("install", function(e) {
  e.waitUntil(
    caches.open(cacheName).then(function(cache) {
      return cache.addAll(filesToCache);
    })
  );
});

/* Serve cached content when offline */
self.addEventListener("fetch", function(e) {
  e.respondWith(
    caches.match(e.request).then(function(response) {
      return response || fetch(e.request);
    })
  );
});
