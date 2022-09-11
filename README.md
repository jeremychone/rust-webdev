Very simple way to serve a local web server from a directory.

Used for dev. Do not use for prod. 

```sh
# Start default port at current dir: http://localhost:8080 at the current folder
webdev

# Starts with custom port: http://localhost:8888
webdev -p 8888

# The root directory to be served (and watched by default)
webdev -d /some/dir

# Overriding the watch paths
webdev -d /some/dir -w /some/dir/dist/js/app-bundle.js -w /some/dir/dist/css

# Start with live mode
#   Will add `<script src="/_webdev_live.js"></script>` at the end of all html file)
#   A web-socket server is always on at /_webdev_live_ws (which send events when root dir file changes)
webdev -l
```