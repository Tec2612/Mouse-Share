// Vanilla JS, no framework/bundler: `withGlobalTauri: true` in
// tauri.conf.json injects `window.__TAURI__`, so commands are called
// directly without an ES module import step that would otherwise need a
// bundler this minimal UI deliberately doesn't have.
const invoke = (...args) => window.__TAURI__.core.invoke(...args);

function $(selector) { return document.querySelector(selector); }
function $all(selector) { return Array.from(document.querySelectorAll(selector)); }

function switchTab(tab) {
  $all(".nav-item").forEach((el) => el.classList.toggle("active", el.dataset.tab === tab));
  $all(".tab").forEach((el) => el.classList.toggle("active", el.id === `tab-${tab}`));
}

$all(".nav-item").forEach((el) => el.addEventListener("click", () => switchTab(el.dataset.tab)));

async function loadDashboard() {
  try {
    const fingerprint = await invoke("get_this_device_fingerprint");
    $("#this-device-fingerprint").textContent = fingerprint;
  } catch (e) {
    console.error("get_this_device_fingerprint failed", e);
  }

  try {
    const paired = await invoke("list_paired_devices");
    const tbody = $("#paired-devices-table tbody");
    tbody.innerHTML = "";
    $("#paired-empty").hidden = paired.length > 0;
    for (const device of paired) {
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td>${escapeHtml(device.name)}</td>
        <td class="fingerprint">${escapeHtml(device.fingerprint)}</td>
        <td><button class="btn danger" data-fp="${escapeHtml(device.fingerprint)}">Remove</button></td>
      `;
      tbody.appendChild(tr);
    }
    tbody.querySelectorAll("button[data-fp]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        await invoke("remove_paired_device", { fingerprint: btn.dataset.fp });
        loadDashboard();
      });
    });
  } catch (e) {
    console.error("list_paired_devices failed", e);
  }

  refreshDiscovery();
}

async function refreshDiscovery() {
  const tbody = $("#discovered-devices-table tbody");
  const empty = $("#discovery-empty");
  empty.hidden = false;
  empty.textContent = "Searching…";
  try {
    const devices = await invoke("discover_devices", { durationMs: 2500 });
    tbody.innerHTML = "";
    empty.hidden = devices.length > 0;
    empty.textContent = "No other Mouse Share devices found on this network yet.";
    for (const device of devices) {
      const tr = document.createElement("tr");
      const addr = device.addrs[0] ?? "";
      tr.innerHTML = `
        <td>${escapeHtml(device.name)}</td>
        <td>${escapeHtml(device.os)}</td>
        <td>${escapeHtml(addr)}:${device.port}</td>
        <td><button class="btn primary" data-addr="${escapeHtml(addr)}" data-port="${device.port}" data-name="${escapeHtml(device.name)}">Pair</button></td>
      `;
      tbody.appendChild(tr);
    }
    tbody.querySelectorAll("button[data-addr]").forEach((btn) => {
      btn.addEventListener("click", () => {
        switchTab("setup");
        beginPairing(btn.dataset.addr, Number(btn.dataset.port), btn.dataset.name);
      });
    });
  } catch (e) {
    empty.textContent = `Discovery failed: ${e}`;
    console.error("discover_devices failed", e);
  }
}
$("#refresh-discovery").addEventListener("click", refreshDiscovery);

async function beginPairing(host, port, name) {
  try {
    const result = await invoke("start_pairing", { addr: host, port, deviceName: name || host });
    $("#pairing-remote-name").textContent = result.remoteName || name || host;
    $("#pairing-sas").textContent = result.sas;
    $("#pairing-card").hidden = false;
  } catch (e) {
    alert(`Could not start pairing: ${e}`);
  }
}

$("#manual-connect-btn").addEventListener("click", () => {
  const host = $("#manual-host").value.trim();
  const port = Number($("#manual-port").value || 45678);
  const name = $("#manual-name").value.trim();
  if (!host) return;
  beginPairing(host, port, name);
});

$("#pairing-confirm-btn").addEventListener("click", async () => {
  try {
    await invoke("confirm_pairing");
    $("#pairing-card").hidden = true;
    loadDashboard();
  } catch (e) {
    alert(`Could not confirm pairing: ${e}`);
  }
});

$("#pairing-cancel-btn").addEventListener("click", async () => {
  await invoke("cancel_pairing");
  $("#pairing-card").hidden = true;
});

// Fires when another device dials into this one to pair (see
// pairing::spawn_pairing_acceptor on the Rust side). Distinct from the
// outgoing pairing-card above: both can be in flight at once.
window.__TAURI__.event.listen("incoming-pairing", (event) => {
  switchTab("setup");
  $("#incoming-pairing-name").textContent = event.payload.remoteName;
  $("#incoming-pairing-sas").textContent = event.payload.sas;
  $("#incoming-pairing-card").hidden = false;
});

$("#incoming-pairing-accept-btn").addEventListener("click", async () => {
  try {
    await invoke("accept_incoming_pairing");
    $("#incoming-pairing-card").hidden = true;
    loadDashboard();
  } catch (e) {
    alert(`Could not accept pairing: ${e}`);
  }
});

$("#incoming-pairing-decline-btn").addEventListener("click", async () => {
  await invoke("decline_incoming_pairing");
  $("#incoming-pairing-card").hidden = true;
});

async function loadLayoutSummary() {
  try {
    const [layout, myId, paired] = await Promise.all([
      invoke("get_layout"),
      invoke("get_this_device_id"),
      invoke("list_paired_devices"),
    ]);
    const edges = layout.edges ?? [];
    // layout.nodes is a Rust HashMap<Uuid, ComputerNode>, which serializes
    // to a JSON *object* keyed by device id, not an array.
    const nodeNames = new Map(Object.values(layout.nodes ?? {}).map((n) => [n.device_id, n.display_name]));
    const el = $("#layout-summary");
    const myEdges = edges.filter((e) => e.from === myId);
    if (myEdges.length === 0) {
      el.textContent = "No edges linked yet — use the form below.";
    } else {
      el.innerHTML = myEdges
        .map((e) => {
          const peerName = escapeHtml(nodeNames.get(e.to) ?? e.to);
          return `My ${e.from_edge} edge ↔ ${peerName}'s ${e.to_edge} edge <button class="btn secondary unlink-btn" data-edge="${e.from_edge}" style="margin-left:8px">Unlink</button>`;
        })
        .join("<br>");
      el.querySelectorAll(".unlink-btn").forEach((btn) => {
        btn.addEventListener("click", async () => {
          await invoke("unlink_layout_edge", { myEdge: btn.dataset.edge });
          loadLayoutSummary();
        });
      });
    }

    const peerSelect = $("#link-peer-device");
    peerSelect.innerHTML = paired.map((d) => `<option value="${escapeHtml(d.fingerprint)}" data-id="${escapeHtml(d.device_id)}">${escapeHtml(d.name)}</option>`).join("");
  } catch (e) {
    console.error("get_layout failed", e);
  }
}

$("#link-edge-btn").addEventListener("click", async () => {
  const peerSelect = $("#link-peer-device");
  const selected = peerSelect.selectedOptions[0];
  if (!selected) {
    alert("Pair a computer first.");
    return;
  }
  const myEdge = $("#link-my-edge").value;
  try {
    await invoke("link_layout_edge", { peerDeviceId: selected.dataset.id, myEdge });
    loadLayoutSummary();
  } catch (e) {
    alert(`Could not link edge: ${e}`);
  }
});

async function loadSettings() {
  try {
    const settings = await invoke("get_settings");
    $("#setting-start-on-boot").checked = settings.start_on_boot;
    $("#setting-auto-discovery").checked = settings.network.auto_discovery_enabled;
    $("#setting-clipboard").checked = settings.clipboard_sharing_enabled;
  } catch (e) {
    console.error("get_settings failed", e);
  }
}

$("#save-settings-btn").addEventListener("click", async () => {
  try {
    const settings = await invoke("get_settings");
    settings.start_on_boot = $("#setting-start-on-boot").checked;
    settings.network.auto_discovery_enabled = $("#setting-auto-discovery").checked;
    settings.clipboard_sharing_enabled = $("#setting-clipboard").checked;
    await invoke("save_settings", { settings });
    const msg = $("#settings-saved-msg");
    msg.hidden = false;
    setTimeout(() => (msg.hidden = true), 2000);
  } catch (e) {
    alert(`Could not save settings: ${e}`);
  }
});

async function loadPermissions() {
  try {
    const status = await invoke("check_permissions");
    setBadge("#perm-accessibility", status.accessibility_granted);
    setBadge("#perm-input-monitoring", status.input_monitoring_granted);
    $("#grant-accessibility-btn").hidden = status.accessibility_granted;
  } catch (e) {
    console.error("check_permissions failed", e);
  }
}

function setBadge(selector, granted) {
  const el = $(selector);
  el.textContent = granted ? "Granted" : "Not granted";
  el.className = `badge ${granted ? "granted" : "denied"}`;
}

$("#grant-accessibility-btn").addEventListener("click", async () => {
  await invoke("request_accessibility");
  setTimeout(loadPermissions, 500);
});

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

loadDashboard();
loadLayoutSummary();
loadSettings();
loadPermissions();
