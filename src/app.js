const LEGACY_STORAGE_KEY = "splanner.tasks.v1";
const ACCOUNT_COLORS = ["#2f6f63", "#4b7fb8", "#d75b62", "#f1b84b", "#7a6fbe", "#bf6b45"];

const state = {
  weekStart: startOfWeek(new Date()),
  tasks: [],
  events: [],
  broadcasts: [],
  accounts: [],
  groups: [],
  adminPassword: "",
  currentUser: sessionStorage.getItem("splanner.currentUser") || "",
  isViewer: false,
  hostClockOffsetMs: 0,
  overviewResetTimer: null,
  taskFilter: sessionStorage.getItem("splanner.taskFilter") || "all",
  lastActivityAt: Date.now(),
  idleRefreshInFlight: false,
};

const IDLE_REFRESH_MS = 60 * 1000;

const loginScreen = document.querySelector("#login-screen");
const appShell = document.querySelector("#app-shell");
const userLogin = document.querySelector("#user-login");
const loginName = document.querySelector("#login-name");
const loginPin = document.querySelector("#login-pin");
const loginMessage = document.querySelector("#login-message");
const grid = document.querySelector("#week-grid");
const weekTitle = document.querySelector("#week-title");
const weekRange = document.querySelector("#week-range");
const hostTime = document.querySelector("#host-time");
const hostDate = document.querySelector("#host-date");
const taskDay = document.querySelector("#task-day");
const taskHour = document.querySelector("#task-hour");
const taskMinute = document.querySelector("#task-minute");
const taskNote = document.querySelector("#task-note");
const eventForm = document.querySelector("#event-form");
const eventTitle = document.querySelector("#event-title");
const eventStartDay = document.querySelector("#event-start-day");
const eventStartHour = document.querySelector("#event-start-hour");
const eventStartMinute = document.querySelector("#event-start-minute");
const eventEndDay = document.querySelector("#event-end-day");
const eventEndHour = document.querySelector("#event-end-hour");
const eventEndMinute = document.querySelector("#event-end-minute");
const eventAssignees = document.querySelector("#event-assignees");
const eventNote = document.querySelector("#event-note");
const createOpen = document.querySelector("#create-open");
const createDialog = document.querySelector("#create-dialog");
const createClose = document.querySelector("#create-close");
const createType = document.querySelector("#create-type");
const taskFilter = document.querySelector("#task-filter");
const broadcastList = document.querySelector("#broadcast-list");
const broadcastForm = document.querySelector("#broadcast-form");
const broadcastMessage = document.querySelector("#broadcast-message");
const broadcastTargets = document.querySelector("#broadcast-targets");
const taskAssignees = document.querySelector("#task-assignees");
const form = document.querySelector("#task-form");
const memberRail = document.querySelector("#member-rail");
const adminDialog = document.querySelector("#admin-dialog");
const adminLogin = document.querySelector("#admin-login");
const adminPanel = document.querySelector("#admin-panel");
const adminPassword = document.querySelector("#admin-password");
const adminMessage = document.querySelector("#admin-message");
const accountForm = document.querySelector("#account-form");
const accountName = document.querySelector("#account-name");
const accountPin = document.querySelector("#account-pin");
const accountList = document.querySelector("#account-list");
const groupForm = document.querySelector("#group-form");
const groupName = document.querySelector("#group-name");
const groupList = document.querySelector("#group-list");

[taskAssignees, eventAssignees, broadcastTargets].forEach((container) => {
  if (!container) return;
  container.addEventListener("change", () => updateGroupSelectionDisabling(container));
});

userLogin.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/user/login", {
    name: loginName.value,
    pin: loginPin.value,
  });
  if (!response.ok) {
    loginMessage.textContent = response.error || "Wrong name or PIN";
    return;
  }
  state.currentUser = response.name;
  state.isViewer = isViewerAccount(response.name);
  sessionStorage.setItem("splanner.currentUser", state.currentUser);
  loginPin.value = "";
  loginMessage.textContent = "";
  await loadTasks();
  await loadEvents();
  await loadBroadcasts();
  showPlanner();
});

document.querySelector("#prev-week").addEventListener("click", () => shiftWeek(-1));
document.querySelector("#next-week").addEventListener("click", () => shiftWeek(1));
document.querySelector("#today").addEventListener("click", () => {
  markOverviewInteraction();
  state.weekStart = startOfWeek(hostNow());
  render();
});

taskFilter.addEventListener("change", () => {
  state.taskFilter = taskFilter.value || "all";
  sessionStorage.setItem("splanner.taskFilter", state.taskFilter);
  markOverviewInteraction();
  renderWeekGrid();
});

createOpen.addEventListener("click", () => openCreateDialog());
createClose.addEventListener("click", () => closeCreateDialog());
createType.addEventListener("change", renderCreateType);

function openCreateDialog() {
  renderCreateType();
  if (typeof createDialog.showModal === "function") {
    createDialog.showModal();
  } else {
    createDialog.setAttribute("open", "");
  }
}

function closeCreateDialog() {
  if (typeof createDialog.close === "function") {
    createDialog.close();
  } else {
    createDialog.removeAttribute("open");
  }
}

function renderCreateType() {
  const type = createType.value || "task";
  form.hidden = type !== "task";
  broadcastForm.hidden = type !== "broadcast";
  eventForm.hidden = type !== "event";
}

broadcastForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer || !state.currentUser) return;
  const message = broadcastMessage.value.trim();
  if (!message) return;

  const response = await apiPost("/api/broadcasts", {
    message,
    targets: getSelectedBroadcastTargets().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
  });
  if (!response.ok) {
    alert(response.error || "Could not send broadcast");
    return;
  }
  state.broadcasts = normalizeBroadcasts(response.broadcasts);
  broadcastForm.reset();
  closeCreateDialog();
  renderBroadcasts();
});

broadcastList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-broadcast-seen]");
  if (!button || state.isViewer || !state.currentUser) return;
  const response = await apiPost("/api/broadcasts/seen", {
    id: button.dataset.broadcastSeen,
    requester: state.currentUser,
  });
  if (!response.ok) {
    alert(response.error || "Could not mark broadcast seen");
    return;
  }
  state.broadcasts = normalizeBroadcasts(response.broadcasts);
  renderBroadcasts();
});

document.querySelector("#admin-open").addEventListener("click", () => openAdmin());
document.querySelector("#login-admin-open").addEventListener("click", () => openAdmin());

document.addEventListener("keydown", (event) => {
  const target = event.target;
  const tagName = target && target.tagName ? target.tagName.toLowerCase() : "";
  if (["input", "textarea", "select", "button"].includes(tagName) || (target && target.isContentEditable)) return;

  if (event.key === "ArrowLeft") {
    event.preventDefault();
    shiftWeek(-1);
  } else if (event.key === "ArrowRight") {
    event.preventDefault();
    shiftWeek(1);
  }
});

["pointerdown", "keydown", "input", "change", "submit"].forEach((eventName) => {
  document.addEventListener(eventName, recordActivity, true);
});

function openAdmin() {
  adminMessage.textContent = "";
  if (typeof adminDialog.showModal === "function") {
    adminDialog.showModal();
  } else {
    adminDialog.setAttribute("open", "");
  }
  if (!state.adminPassword) adminPassword.focus();
}

document.querySelector("#admin-close").addEventListener("click", () => {
  if (typeof adminDialog.close === "function") {
    adminDialog.close();
  } else {
    adminDialog.removeAttribute("open");
  }
});

document.querySelector("#admin-logout").addEventListener("click", () => {
  state.adminPassword = "";
  adminPassword.value = "";
  setAdminMode(false);
});

adminLogin.addEventListener("submit", async (event) => {
  event.preventDefault();
  const password = adminPassword.value;
  const response = await apiPost("/api/admin/login", { password });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Wrong password";
    return;
  }
  state.adminPassword = password;
  adminMessage.textContent = "";
  setAdminMode(true);
});

accountForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/accounts", {
    password: state.adminPassword,
    name: accountName.value,
    pin: accountPin.value,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not add account";
    return;
  }
  accountName.value = "";
  accountPin.value = "";
  await loadAccounts(response.accounts);
  await loadGroups();
});

accountList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-account-delete]");
  if (!button) return;
  const response = await apiPost("/api/accounts/delete", {
    password: state.adminPassword,
    name: button.dataset.accountDelete,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not delete account";
    return;
  }
  await loadAccounts(response.accounts);
  await loadGroups();
});

groupForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const response = await apiPost("/api/groups", {
    password: state.adminPassword,
    name: groupName.value,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not add group";
    return;
  }
  groupName.value = "";
  await loadGroups(response.groups);
});

groupList.addEventListener("click", async (event) => {
  const deleteButton = event.target.closest("[data-group-delete]");
  if (deleteButton) {
    const response = await apiPost("/api/groups/delete", {
      password: state.adminPassword,
      name: deleteButton.dataset.groupDelete,
    });
    if (!response.ok) {
      adminMessage.textContent = response.error || "Could not delete group";
      return;
    }
    await loadGroups(response.groups);
    return;
  }

  const memberButton = event.target.closest("[data-group-member]");
  if (!memberButton) return;
  const response = await apiPost("/api/groups/member", {
    password: state.adminPassword,
    group: memberButton.dataset.groupName,
    member: memberButton.dataset.groupMember,
    action: memberButton.dataset.groupAction,
  });
  if (!response.ok) {
    adminMessage.textContent = response.error || "Could not update group";
    return;
  }
  await loadGroups(response.groups);
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer) return;
  const data = new FormData(form);
  const title = String(data.get("title") || "").trim();
  if (!title) return;
  const time = selectedTimeValue(taskHour, taskMinute, "by");
  if (time === null) return;

  const response = await apiPost("/api/tasks", {
    title,
    date: data.get("day"),
    time,
    assignees: getSelectedAssignees().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
    note: String(data.get("note") || "").trim(),
  });
  if (!response.ok) {
    alert(response.error || "Could not add task");
    return;
  }
  state.tasks = normalizeTasks(response.tasks);

  form.reset();
  taskDay.value = toDateKey(hostNow());
  setDefaultTaskTime();
  closeCreateDialog();
  render();
});

eventForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (state.isViewer || !state.currentUser) return;
  const data = new FormData(eventForm);
  const title = String(data.get("title") || "").trim();
  if (!title) return;
  const startTime = selectedTimeValue(eventStartHour, eventStartMinute, "start");
  const endTime = selectedTimeValue(eventEndHour, eventEndMinute, "end");
  if (startTime === null || endTime === null || !startTime || !endTime) return;

  const response = await apiPost("/api/events", {
    title,
    startDate: data.get("startDay"),
    startTime,
    endDate: data.get("endDay"),
    endTime,
    assignees: getSelectedEventAssignees().join(","),
    requester: state.currentUser,
    createdAt: new Date().toISOString(),
    note: String(data.get("note") || "").trim(),
  });
  if (!response.ok) {
    alert(response.error || "Could not add event");
    return;
  }
  state.events = normalizeEvents(response.events);
  eventForm.reset();
  setDefaultEventTime();
  closeCreateDialog();
  render();
});

grid.addEventListener("click", async (event) => {
  const noteButton = event.target.closest("[data-note-toggle]");
  if (noteButton) {
    const panel = document.querySelector(`#${CSS.escape(noteButton.dataset.noteToggle)}`);
    if (panel) panel.hidden = !panel.hidden;
    return;
  }

  const eventNoteButton = event.target.closest("[data-event-note-toggle]");
  if (eventNoteButton) {
    const panel = document.querySelector(`#${CSS.escape(eventNoteButton.dataset.eventNoteToggle)}`);
    if (panel) panel.hidden = !panel.hidden;
    return;
  }

  const eventDeleteButton = event.target.closest("[data-event-delete]");
  if (eventDeleteButton) {
    const response = await apiPost("/api/events/delete", {
      id: eventDeleteButton.dataset.eventDelete,
      requester: state.currentUser,
    });
    if (!response.ok) {
      alert(response.error || "You cannot delete this event");
      return;
    }
    state.events = normalizeEvents(response.events);
    await loadEvents(false);
    render();
    return;
  }

  const button = event.target.closest("[data-delete]");
  if (!button) return;
  const response = await apiPost("/api/tasks/delete", {
    id: button.dataset.delete,
    requester: state.currentUser,
  });
  if (!response.ok) {
    alert(response.error || "You cannot delete this task");
    return;
  }
  state.tasks = normalizeTasks(response.tasks);
  render();
});

let swipeStartX = 0;
let swipeStartY = 0;

grid.addEventListener("pointerdown", (event) => {
  swipeStartX = event.clientX;
  swipeStartY = event.clientY;
});

grid.addEventListener("pointerup", (event) => {
  const xDelta = event.clientX - swipeStartX;
  const yDelta = event.clientY - swipeStartY;
  if (Math.abs(xDelta) < 110 || Math.abs(xDelta) < Math.abs(yDelta) * 1.3) return;
  shiftWeek(xDelta < 0 ? 1 : -1);
});

async function init() {
  renderClock();
  await syncHostTime();
  state.weekStart = startOfWeek(hostNow());
  renderClock();
  setInterval(renderClock, 1000);
  setInterval(syncHostTime, 5 * 60 * 1000);
  setInterval(refreshPlannerDataIfIdle, 5 * 1000);
  renderTimeSelectors();
  await loadAccounts();
  await loadGroups();
  await loadTasks();
  await loadEvents();
  await loadBroadcasts();
  if (state.currentUser && state.accounts.includes(state.currentUser)) {
    state.isViewer = isViewerAccount(state.currentUser);
    showPlanner();
  } else {
    showLogin();
  }
  render();
}

async function loadTasks(shouldRender = true) {
  try {
    state.tasks = normalizeTasks(await fetch("/api/tasks", { cache: "no-store" }).then((response) => response.json()));
    if (!state.tasks.length) {
      await migrateLegacyTasks();
    }
  } catch {
    state.tasks = [];
  }
  if (shouldRender) renderWeekGrid();
}

async function loadEvents(shouldRender = true) {
  try {
    state.events = normalizeEvents(await fetch("/api/events", { cache: "no-store" }).then((response) => response.json()));
  } catch {
    state.events = [];
  }
  if (shouldRender) renderWeekGrid();
}

async function loadBroadcasts(shouldRender = true) {
  try {
    state.broadcasts = normalizeBroadcasts(await fetch("/api/broadcasts", { cache: "no-store" }).then((response) => response.json()));
  } catch {
    state.broadcasts = [];
  }
  if (shouldRender) renderBroadcasts();
}

async function refreshPlannerDataIfIdle() {
  if (!state.currentUser || appShell.hidden || state.idleRefreshInFlight) return;
  if (document.hidden) return;
  if (Date.now() - state.lastActivityAt < IDLE_REFRESH_MS) return;

  state.idleRefreshInFlight = true;
  try {
    await loadTasks(false);
    await loadEvents(false);
    await loadBroadcasts(false);
    renderWeekGrid();
    renderBroadcasts();
    state.lastActivityAt = Date.now();
  } finally {
    state.idleRefreshInFlight = false;
  }
}

function recordActivity() {
  state.lastActivityAt = Date.now();
}

async function migrateLegacyTasks() {
  let legacyTasks = [];
  try {
    legacyTasks = JSON.parse(localStorage.getItem(LEGACY_STORAGE_KEY) || "[]") || [];
  } catch {
    legacyTasks = [];
  }
  if (!legacyTasks.length) return;

  for (const task of legacyTasks) {
    await apiPost("/api/tasks", {
      title: task.title || "Untitled",
      date: task.date || toDateKey(hostNow()),
      time: task.time || "",
      assignees: normalizeAssignees(task).join(","),
      requester: task.requester && !isViewerAccount(task.requester) ? task.requester : state.currentUser,
      createdAt: task.createdAt || new Date().toISOString(),
      note: task.note || task.notes || "",
    });
  }
  state.tasks = normalizeTasks(await fetch("/api/tasks", { cache: "no-store" }).then((response) => response.json()));
  localStorage.setItem(`${LEGACY_STORAGE_KEY}.migrated`, new Date().toISOString());
}

async function loadGroups(groups = null) {
  state.groups = groups || await fetch("/api/groups").then((response) => response.json());
  renderGroupControls();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
}

async function syncHostTime() {
  try {
    const response = await fetch("/api/now");
    const data = await response.json();
    state.hostClockOffsetMs = data.nowMs - Date.now();
  } catch {
    state.hostClockOffsetMs = 0;
  }
}

function hostNow() {
  return new Date(Date.now() + state.hostClockOffsetMs);
}

function renderClock() {
  const now = hostNow();
  hostTime.textContent = formatDate(now, { hour: "2-digit", minute: "2-digit", hour12: false });
  hostDate.textContent = formatDate(now, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

async function loadAccounts(accounts = null) {
  state.accounts = accounts || await fetch("/api/accounts").then((response) => response.json());
  if (!state.accounts.includes(state.currentUser)) {
    state.currentUser = "";
    state.isViewer = false;
    sessionStorage.removeItem("splanner.currentUser");
  }
  renderLoginOptions();
  renderMembers();
  renderAccountControls();
  renderGroupControls();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
}

function render() {
  renderMembers();
  renderWeekHeading();
  renderDayOptions();
  renderPersonOptions();
  renderBroadcastTargets();
  renderTaskFilter();
  renderWeekGrid();
  renderBroadcasts();
  renderAccountControls();
  renderGroupControls();
}

function showLogin() {
  loginScreen.hidden = false;
  appShell.hidden = true;
  renderLoginOptions();
}

function showPlanner() {
  state.isViewer = isViewerAccount(state.currentUser);
  appShell.classList.toggle("viewer-mode", state.isViewer);
  loginScreen.hidden = false;
  loginScreen.hidden = true;
  appShell.hidden = false;
  render();
}

function isViewerAccount(name) {
  const normalized = String(name).trim().toLowerCase();
  return normalized === "viewer" || normalized === "overview";
}

function isSystemAccount(name) {
  return String(name).trim().toLowerCase() === "overview";
}

function taskAccounts() {
  return state.accounts.filter((name) => !isSystemAccount(name));
}

function renderLoginOptions() {
  loginName.innerHTML = state.accounts.length
    ? state.accounts.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join("")
    : `<option value="">Ask admin to add accounts</option>`;
  loginName.value = state.accounts.includes(state.currentUser) ? state.currentUser : state.accounts[0] || "";
}

function renderMembers() {
  memberRail.innerHTML = taskAccounts().map((name, index) => `
    <article class="member ${sameName(name, state.currentUser) ? "current-member" : ""}">
      <span class="avatar" style="background:${ACCOUNT_COLORS[index % ACCOUNT_COLORS.length]}">${escapeHtml(name.slice(0, 1))}</span>
      <strong>${escapeHtml(name)}</strong>
    </article>
  `).join("");
}

function renderAccountControls() {
  accountList.innerHTML = state.accounts.map((name) => `
    <div class="account-row">
      <span>${escapeHtml(name)}</span>
      ${isSystemAccount(name)
        ? `<span class="chip">Built in</span>`
        : `<button type="button" data-account-delete="${escapeHtml(name)}">Delete</button>`}
    </div>
  `).join("");
}

function renderGroupControls() {
  groupList.innerHTML = state.groups.length
    ? state.groups.map((group) => `
      <article class="group-row">
        <header>
          <strong>${escapeHtml(group.name)}</strong>
          <button type="button" data-group-delete="${escapeHtml(group.name)}">Delete</button>
        </header>
        <div class="group-members">
          ${taskAccounts().map((name) => {
            const isMember = group.members.includes(name);
            return `
              <button type="button"
                data-group-name="${escapeHtml(group.name)}"
                data-group-member="${escapeHtml(name)}"
                data-group-action="${isMember ? "remove" : "add"}">
                ${isMember ? "Remove" : "Add"} ${escapeHtml(name)}
              </button>
            `;
          }).join("")}
        </div>
      </article>
    `).join("")
    : `<div class="empty-day">No groups yet</div>`;
}

function renderPersonOptions() {
  const optionsHtml = renderSelectableTargets();
  taskAssignees.innerHTML = optionsHtml;
  updateGroupSelectionDisabling(taskAssignees);
  if (eventAssignees) {
    eventAssignees.innerHTML = optionsHtml;
    updateGroupSelectionDisabling(eventAssignees);
  }
}

function renderBroadcastTargets() {
  if (!broadcastTargets) return;
  broadcastTargets.innerHTML = renderSelectableTargets();
  updateGroupSelectionDisabling(broadcastTargets);
}

function renderSelectableTargets() {
  const people = taskAccounts();
  const personOptions = people.map((name) => `
      <label class="person-option user-option">
        <input type="checkbox" value="${escapeHtml(name)}">
        <span class="option-kind">Person</span>
        <span class="option-name">${escapeHtml(name)}</span>
      </label>
    `).join("");
  const groupOptions = state.groups.map((group) => `
      <label class="person-option group-option">
        <input type="checkbox" value="@${escapeHtml(group.name)}">
        <span class="option-kind">Group</span>
        <span class="option-name">${escapeHtml(group.name)}</span>
      </label>
    `).join("");

  if (!people.length && !state.groups.length) {
    return `<span class="chip">No accounts or groups yet</span>`;
  }

  return `
    ${people.length ? `
      <section class="option-section target-users" aria-label="People">
        <span class="option-heading">People</span>
        <div class="option-items">${personOptions}</div>
      </section>
    ` : ""}
    ${state.groups.length ? `
      <section class="option-section target-groups" aria-label="Groups">
        <span class="option-heading">Groups</span>
        <div class="option-items">${groupOptions}</div>
      </section>
    ` : ""}
  `;
}

function getSelectedBroadcastTargets() {
  return getSelectableCheckedValues(broadcastTargets);
}

function renderTaskFilter() {
  const people = taskAccounts();
  const options = [
    { value: "all", label: "Everyone" },
    ...people.map((name) => ({ value: `person:${name}`, label: name })),
    ...state.groups.map((group) => ({ value: `group:${group.name}`, label: group.name })),
  ];

  if (!options.some((option) => option.value === state.taskFilter)) {
    state.taskFilter = "all";
  }
  taskFilter.innerHTML = `
    <option value="all">Everyone</option>
    ${people.length ? `
      <optgroup label="People">
        ${people.map((name) => `<option value="person:${escapeHtml(name)}">Person: ${escapeHtml(name)}</option>`).join("")}
      </optgroup>
    ` : ""}
    ${state.groups.length ? `
      <optgroup label="Groups">
        ${state.groups.map((group) => `<option value="group:${escapeHtml(group.name)}">Group: ${escapeHtml(group.name)}</option>`).join("")}
      </optgroup>
    ` : ""}
  `;
  taskFilter.value = state.taskFilter;
}

function getSelectableCheckedValues(container) {
  if (!container) return [];
  return Array.from(container.querySelectorAll("input:checked:not(:disabled)")).map((input) => input.value);
}

function getSelectedAssignees() {
  return getSelectableCheckedValues(taskAssignees);
}

function getSelectedEventAssignees() {
  return getSelectableCheckedValues(eventAssignees);
}

function updateGroupSelectionDisabling(container) {
  if (!container) return;
  const selectedGroups = Array.from(container.querySelectorAll('input:checked'))
    .map((input) => input.value)
    .filter((value) => value.startsWith('@'))
    .map((value) => value.slice(1));
  const coveredPeople = new Set();
  selectedGroups.forEach((groupName) => {
    resolveGroupMembers(groupName).forEach((member) => coveredPeople.add(member.toLowerCase()));
  });

  Array.from(container.querySelectorAll('input')).forEach((input) => {
    const isPerson = !input.value.startsWith('@');
    const shouldDisable = isPerson && coveredPeople.has(input.value.toLowerCase());
    input.disabled = shouldDisable;
    if (shouldDisable) input.checked = false;
    const label = input.closest('.person-option');
    if (label) label.classList.toggle('is-disabled', shouldDisable);
  });
}

function resolveGroupMembers(groupName, seen = new Set()) {
  const key = String(groupName || '').toLowerCase();
  if (!key || seen.has(key)) return [];
  seen.add(key);
  const group = state.groups.find((candidate) => sameName(candidate.name, groupName));
  if (!group) return [];

  const members = [];
  const addMember = (value) => {
    const member = String(value || '').trim();
    if (!member) return;
    if (member.startsWith('@')) {
      resolveGroupMembers(member.slice(1), seen).forEach((nested) => members.push(nested));
    } else {
      members.push(member);
    }
  };

  (group.members || []).forEach(addMember);
  (group.subgroups || group.children || group.groups || []).forEach((nestedGroup) => {
    if (typeof nestedGroup === 'string') {
      resolveGroupMembers(nestedGroup.replace(/^@/, ''), seen).forEach((nested) => members.push(nested));
    } else if (nestedGroup && nestedGroup.name) {
      resolveGroupMembers(nestedGroup.name, seen).forEach((nested) => members.push(nested));
    }
  });

  return Array.from(new Set(members.map((member) => {
    const account = taskAccounts().find((name) => sameName(name, member));
    return account || member;
  })));
}

function setAdminMode(isLoggedIn) {
  adminLogin.hidden = isLoggedIn;
  adminPanel.hidden = !isLoggedIn;
  renderAccountControls();
}

function renderBroadcasts() {
  if (!broadcastList) return;
  const broadcasts = visibleBroadcasts();
  broadcastList.innerHTML = broadcasts.map(renderBroadcast).join("");
}

function renderBroadcast(broadcast) {
  const targets = normalizeBroadcastTargets(broadcast);
  const targetLabel = targets.length ? targets.join(", ") : "Everyone";
  const canClose = !state.isViewer && broadcastAppliesToPerson(broadcast, state.currentUser) && !hasSeenBroadcast(broadcast, state.currentUser);
  return `
    <article class="broadcast-card">
      <header>
        <strong>${escapeHtml(broadcast.requester || "Someone")} broadcasts</strong>
        ${canClose ? `<button class="broadcast-seen" type="button" data-broadcast-seen="${escapeHtml(broadcast.id)}" aria-label="Close broadcast">&times;</button>` : ""}
      </header>
      <p class="broadcast-message">${linkifyNote(broadcast.message || "")}</p>
      <div class="broadcast-meta">
        <span class="chip">To: ${escapeHtml(targetLabel)}</span>
      </div>
    </article>
  `;
}

function visibleBroadcasts() {
  return state.broadcasts.filter((broadcast) => {
    if (state.isViewer) return !broadcastIsComplete(broadcast);
    return broadcastAppliesToPerson(broadcast, state.currentUser) && !hasSeenBroadcast(broadcast, state.currentUser);
  });
}

function renderWeekHeading() {
  const days = getWeekDays();
  const todayStart = startOfWeek(hostNow());
  const weekOffset = Math.round((state.weekStart - todayStart) / (7 * 24 * 60 * 60 * 1000));
  weekTitle.textContent = weekOffset === 0 ? "This week" : weekOffset === 1 ? "Next week" : weekOffset === -1 ? "Last week" : `${Math.abs(weekOffset)} weeks ${weekOffset > 0 ? "ahead" : "back"}`;
  weekRange.textContent = `${formatDate(days[0], { month: "long", day: "numeric" })} - ${formatDate(days[6], { month: "long", day: "numeric", year: "numeric" })}`;
}

function renderDayOptions() {
  const options = getWeekDays().map((day) => `
    <option value="${toDateKey(day)}">${formatDate(day, { weekday: "long", month: "short", day: "numeric" })}</option>
  `).join("");
  const currentTask = taskDay.value || toDateKey(hostNow());
  taskDay.innerHTML = options;
  taskDay.value = getWeekDays().some((day) => toDateKey(day) === currentTask) ? currentTask : toDateKey(getWeekDays()[0]);

  [eventStartDay, eventEndDay].forEach((select) => {
    if (!select) return;
    const current = select.value || toDateKey(hostNow());
    select.innerHTML = options;
    select.value = getWeekDays().some((day) => toDateKey(day) === current) ? current : toDateKey(getWeekDays()[0]);
  });
}

function renderWeekGrid() {
  grid.innerHTML = getWeekDays().map((day) => {
    const key = toDateKey(day);
    const tasks = state.tasks
      .filter((task) => task.date === key)
      .filter(taskMatchesCurrentFilter)
      .sort(sortTasks);
    const events = state.events
      .filter((event) => eventOverlapsDay(event, key))
      .filter(eventMatchesCurrentFilter)
      .sort(sortEvents);
    const itemsHtml = [
      ...events.map(renderEvent),
      ...tasks.map(renderTask),
    ].join("");
    return `
      <article class="day-column">
        <header class="day-header">
          <strong>${formatDate(day, { weekday: "short" })}</strong>
          <span class="date-label">${formatDate(day, { month: "short", day: "numeric" })}</span>
        </header>
        <div class="task-list">
          ${itemsHtml || `<div class="empty-day">Open</div>`}
        </div>
      </article>
    `;
  }).join("");
}

function renderTask(task, index) {
  const assignees = normalizeAssignees(task);
  const assignee = assignees.length ? assignees.join(", ") : "Anyone";
  const requester = task.requester ? `by: ${task.requester}` : "by: unknown";
  const note = String(task.note || task.notes || "").trim();
  const noteId = `task-note-${String(task.id).replace(/[^a-zA-Z0-9_-]/g, "-")}`;
  const canDelete = canDeleteTask(task);
  return `
    <article class="task-card ${note ? "has-note" : ""}" data-tone="${index % 4}">
      <p class="task-title">${escapeHtml(task.title)}</p>
      <div class="task-meta">
        ${task.time ? `<span class="chip time-chip">${formatTaskTime(task.time)}</span>` : ""}
        <span class="chip">${escapeHtml(assignee)}</span>
        <span class="chip">${escapeHtml(requester)}</span>
      </div>
      <div class="task-actions">
        <button class="note-toggle" type="button" data-note-toggle="${escapeHtml(noteId)}" aria-label="Show note for ${escapeHtml(task.title)}">&#8942;</button>
        ${canDelete ? `<button class="delete-task" type="button" data-delete="${escapeHtml(task.id)}" aria-label="Remove ${escapeHtml(task.title)}">&times;</button>` : ""}
      </div>
      <div id="${escapeHtml(noteId)}" class="task-note-panel" hidden>${note ? linkifyNote(note) : ""}</div>
    </article>
  `;
}

function renderEvent(event, index) {
  const assignees = normalizeAssignees(event);
  const assignee = assignees.length ? assignees.join(", ") : "Anyone";
  const requester = event.requester ? `by: ${event.requester}` : "by: unknown";
  const note = String(event.note || event.notes || "").trim();
  const noteId = `event-note-${String(event.id).replace(/[^a-zA-Z0-9_-]/g, "-")}`;
  const canDelete = canDeleteEvent(event);
  return `
    <article class="event-card ${note ? "has-note" : ""}" data-tone="${index % 4}">
      <p class="event-title">${escapeHtml(event.title)}</p>
      <div class="event-meta">
        <span class="chip time-chip">${escapeHtml(formatEventTime(event))}</span>
        <span class="chip">${escapeHtml(assignee)}</span>
        <span class="chip">${escapeHtml(requester)}</span>
      </div>
      <div class="event-actions">
        <button class="event-note-toggle" type="button" data-event-note-toggle="${escapeHtml(noteId)}" aria-label="Show note for ${escapeHtml(event.title)}">&#8942;</button>
        ${canDelete ? `<button class="delete-event" type="button" data-event-delete="${escapeHtml(event.id)}" aria-label="Remove ${escapeHtml(event.title)}">&times;</button>` : ""}
      </div>
      <div id="${escapeHtml(noteId)}" class="event-note-panel" hidden>${note ? linkifyNote(note) : ""}</div>
    </article>
  `;
}

function normalizeEvents(events) {
  return Array.isArray(events) ? events.map((event) => ({
    ...event,
    assignees: normalizeAssignees(event),
    note: event.note || event.notes || "",
  })) : [];
}

function normalizeBroadcasts(broadcasts) {
  return Array.isArray(broadcasts) ? broadcasts.map((broadcast) => ({
    ...broadcast,
    targets: normalizeBroadcastTargets(broadcast),
    seenBy: normalizeSeenBy(broadcast),
  })) : [];
}

function normalizeBroadcastTargets(broadcast) {
  if (Array.isArray(broadcast.targets)) return broadcast.targets;
  if (typeof broadcast.targets === "string") return broadcast.targets.split(",").map((item) => item.trim()).filter(Boolean);
  return [];
}

function normalizeSeenBy(broadcast) {
  if (Array.isArray(broadcast.seenBy)) return broadcast.seenBy;
  if (Array.isArray(broadcast.seen_by)) return broadcast.seen_by;
  if (typeof broadcast.seenBy === "string") return broadcast.seenBy.split(",").map((item) => item.trim()).filter(Boolean);
  return [];
}

function hasSeenBroadcast(broadcast, name) {
  return normalizeSeenBy(broadcast).some((seenName) => sameName(seenName, name));
}

function broadcastTargetPeople(broadcast) {
  const targets = normalizeBroadcastTargets(broadcast);
  const people = [];
  const addPerson = (name) => {
    if (!name || isSystemAccount(name)) return;
    if (!people.some((existing) => sameName(existing, name))) people.push(name);
  };

  if (!targets.length) {
    taskAccounts().forEach(addPerson);
    return people;
  }

  targets.forEach((target) => {
    if (target.startsWith("@")) {
      const group = state.groups.find((candidate) => sameName(candidate.name, target.slice(1)));
      if (group) group.members.forEach(addPerson);
    } else {
      addPerson(target);
    }
  });
  return people;
}

function broadcastAppliesToPerson(broadcast, person) {
  return broadcastTargetPeople(broadcast).some((name) => sameName(name, person));
}

function broadcastIsComplete(broadcast) {
  const people = broadcastTargetPeople(broadcast);
  return people.length > 0 && people.every((name) => hasSeenBroadcast(broadcast, name));
}

function taskMatchesCurrentFilter(task) {
  const filter = state.taskFilter || "all";
  if (filter === "all") return true;
  const assignees = normalizeAssignees(task);
  if (filter.startsWith("person:")) {
    const person = filter.slice("person:".length);
    return taskAppliesToPerson(task, person);
  }
  if (filter.startsWith("group:")) {
    const group = filter.slice("group:".length);
    return assignees.some((assignee) => sameName(assignee, `@${group}`));
  }
  return true;
}

function taskAppliesToPerson(task, person) {
  const assignees = normalizeAssignees(task);
  if (!assignees.length) return true;
  return assignees.some((assignee) => {
    if (assignee.startsWith("@")) {
      const group = state.groups.find((candidate) => sameName(candidate.name, assignee.slice(1)));
      return group ? group.members.some((member) => sameName(member, person)) : false;
    }
    return sameName(assignee, person);
  });
}

function canDeleteTask(task) {
  if (state.isViewer || !state.currentUser) return false;
  if (sameName(task.requester, state.currentUser)) return true;
  const assignees = normalizeAssignees(task);
  if (!assignees.length) return true;
  return taskAppliesToPerson(task, state.currentUser);
}

function eventMatchesCurrentFilter(event) {
  return taskMatchesCurrentFilter(event);
}

function canDeleteEvent(event) {
  if (state.isViewer || !state.currentUser) return false;
  if (sameName(event.requester, state.currentUser)) return true;
  const assignees = normalizeAssignees(event);
  if (!assignees.length) return true;
  return taskAppliesToPerson(event, state.currentUser);
}

function eventOverlapsDay(event, dateKey) {
  const start = String(event.startDate || "");
  const end = String(event.endDate || start);
  return start <= dateKey && dateKey <= end;
}

function sortEvents(a, b) {
  const aTime = a.startTime || "";
  const bTime = b.startTime || "";
  if (aTime && bTime && aTime !== bTime) return aTime.localeCompare(bTime);
  return String(a.createdAt || "").localeCompare(String(b.createdAt || ""));
}

function sortTasks(a, b) {
  if (a.time && b.time && a.time !== b.time) return a.time.localeCompare(b.time);
  if (a.time && !b.time) return -1;
  if (!a.time && b.time) return 1;
  return String(a.createdAt || "").localeCompare(String(b.createdAt || ""));
}

async function apiPost(url, payload) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  const data = await response.json();
  return { ...data, ok: response.ok && data.ok };
}

function shiftWeek(amount) {
  markOverviewInteraction();
  state.weekStart = addDays(state.weekStart, amount * 7);
  render();
}

function markOverviewInteraction() {
  if (!state.isViewer) return;
  if (state.overviewResetTimer) {
    clearTimeout(state.overviewResetTimer);
  }
  state.overviewResetTimer = setTimeout(() => {
    if (!state.isViewer) return;
    state.weekStart = startOfWeek(hostNow());
    render();
  }, 10 * 60 * 1000);
}

function renderTimeSelectors() {
  [eventStartMinute, eventEndMinute].forEach((select) => {
    if (!select) return;
    select.innerHTML = Array.from({ length: 12 }, (_, index) => {
      const value = String(index * 5).padStart(2, "0");
      return `<option value="${value}">${value}</option>`;
    }).join("");
  });
}

function selectedTimeValue(hourInput, minuteSelect, label) {
  if (!hourInput || !minuteSelect) return "";
  const rawHour = hourInput.value.trim();
  if (!rawHour) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }
  if (!/^\d{1,2}$/.test(rawHour)) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }
  const hour = Number(rawHour);
  if (hour < 0 || hour > 23) {
    alert(`Use a ${label} hour between 0 and 23.`);
    hourInput.focus();
    return null;
  }

  const rawMinute = String(minuteSelect.value || "00").trim();
  if (!/^\d{1,2}$/.test(rawMinute)) {
    alert(`Use a ${label} minute between 0 and 59.`);
    minuteSelect.focus();
    return null;
  }
  const minute = Number(rawMinute);
  if (minute < 0 || minute > 59) {
    alert(`Use a ${label} minute between 0 and 59.`);
    minuteSelect.focus();
    return null;
  }

  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

function setDefaultTaskTime() {
  if (taskHour) taskHour.value = "";
  if (taskMinute) taskMinute.value = "00";
  if (taskNote) taskNote.value = "";
}

function setDefaultEventTime() {
  if (eventStartHour) eventStartHour.value = "";
  if (eventEndHour) eventEndHour.value = "";
  if (eventStartMinute) eventStartMinute.value = "00";
  if (eventEndMinute) eventEndMinute.value = "00";
  if (eventNote) eventNote.value = "";
}

function getWeekDays() {
  return Array.from({ length: 7 }, (_, index) => addDays(state.weekStart, index));
}

function startOfWeek(date) {
  const copy = new Date(date);
  copy.setHours(0, 0, 0, 0);
  const day = copy.getDay();
  const mondayOffset = day === 0 ? -6 : 1 - day;
  return addDays(copy, mondayOffset);
}

function addDays(date, amount) {
  const copy = new Date(date);
  copy.setDate(copy.getDate() + amount);
  return copy;
}

function toDateKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatDate(date, options) {
  return new Intl.DateTimeFormat(undefined, options).format(date);
}

function formatTaskTime(value) {
  const [hour, minute] = String(value).split(":").map(Number);
  const date = new Date();
  date.setHours(hour, minute || 0, 0, 0);
  return formatDate(date, { hour: "2-digit", minute: "2-digit", hour12: false });
}

function formatEventTime(event) {
  const startDate = event.startDate || "";
  const endDate = event.endDate || startDate;
  const start = `${startDate} ${event.startTime || ""}`.trim();
  const end = `${endDate} ${event.endTime || ""}`.trim();
  return start === end ? start : `${start} - ${end}`;
}

function normalizeTasks(tasks) {
  return Array.isArray(tasks) ? tasks.map((task) => ({
    ...task,
    assignees: normalizeAssignees(task),
    note: task.note || task.notes || "",
  })) : [];
}

function normalizeAssignees(task) {
  if (Array.isArray(task.assignees)) return task.assignees;
  if (typeof task.assignees === "string") return task.assignees.split(",").map((item) => item.trim()).filter(Boolean);
  if (task.assignee) return [task.assignee];
  return [];
}

function linkifyNote(value) {
  const escaped = escapeHtml(value);
  return escaped.replace(/(https?:\/\/[^\s<]+)/g, '<a href="$1" target="_blank" rel="noopener noreferrer">$1</a>');
}

function sameName(left, right) {
  return String(left || "").trim().toLowerCase() === String(right || "").trim().toLowerCase();
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

init();
