'use strict';

const { hostname, port } = window.location;
const ws_url = `ws://${hostname}:${port ?? 80}/_webdev_live_ws`;

const socket = new WebSocket(ws_url);

// Connection opened
socket.addEventListener('open', function (event) {
});

// Listen for messages
socket.addEventListener('message', function (event) {
  document.location.reload()
});