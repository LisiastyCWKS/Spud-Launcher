/*
 * hello-spud — reference plugin for Spud Launcher (plugin API 1).
 *
 * Copy this folder into `<data>/plugins/hello-spud/`, open Settings → Plugins,
 * flip the toggle on. The panel appears under Plugins in the sidebar.
 *
 * Everything here is plain browser JavaScript: no imports, no bundler, no
 * Node APIs. `Spud` is the whole interface the launcher exposes.
 */
(function () {
  var launches = Spud.storage.get("launches", 0);
  var lastEvent = Spud.storage.get("lastEvent", "nothing yet");

  Spud.log("loaded at", new Date().toISOString());

  Spud.events.on("game:started", function (payload) {
    launches += 1;
    lastEvent = "game started: " + payload.instanceId;
    Spud.storage.set("launches", launches);
    Spud.storage.set("lastEvent", lastEvent);
    Spud.ui.notify("Launch #" + launches + " — " + payload.instanceId);
    render();
  });

  Spud.events.on("game:stopped", function (payload) {
    lastEvent = "game stopped: " + payload.instanceId;
    Spud.storage.set("lastEvent", lastEvent);
    render();
  });

  Spud.events.on("instances:changed", function (payload) {
    lastEvent = payload.count + " instance(s) installed";
    Spud.storage.set("lastEvent", lastEvent);
    render();
  });

  var host = null;

  function render() {
    if (!host) return;
    host.innerHTML = "";

    var heading = document.createElement("h4");
    heading.textContent = "Hello from a plugin";
    host.appendChild(heading);

    var intro = document.createElement("p");
    intro.textContent =
      "This panel is plain DOM created by main.js. Nothing in the launcher's React tree knows it exists.";
    host.appendChild(intro);

    var list = document.createElement("ul");
    list.style.margin = "0 0 10px";
    list.style.paddingLeft = "18px";

    var rows = ["Launches counted: " + launches, "Last event: " + lastEvent];
    for (var index = 0; index < rows.length; index += 1) {
      var item = document.createElement("li");
      item.textContent = rows[index];
      list.appendChild(item);
    }
    host.appendChild(list);

    var button = document.createElement("button");
    button.type = "button";
    button.className = "btn btn-sm";
    button.textContent = "Reset counter";
    button.addEventListener("click", function () {
      launches = 0;
      Spud.storage.set("launches", launches);
      render();
    });
    host.appendChild(button);
  }

  Spud.ui.panel({
    id: "counter",
    title: "Hello Spud",
    mount: function (element) {
      host = element;
      render();
      return function () {
        host = null;
      };
    },
  });
})();
