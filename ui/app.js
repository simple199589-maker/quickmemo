const { invoke } = window.__TAURI__.core;

const REPO_URL = 'https://github.com/simple199589-maker/quickmemo';

const $ = (id) => document.getElementById(id);

const elements = {
  sidebar: $('sidebar'),
  search: $('search'),
  refresh: $('refresh'),
  settingsOpen: $('settings-open'),
  quickAdd: $('quick-add'),
  breadcrumb: $('breadcrumb'),
  cards: $('cards'),
  addGroup: $('add-group'),
  githubLink: $('github-link'),
  settingsBackdrop: $('settings-backdrop'),
  settingsDrawer: $('settings-drawer'),
  settingsClose: $('settings-close'),
  autostart: $('autostart'),
  startHidden: $('start-hidden'),
  shortcutRecord: $('shortcut-record'),
  shortcutClear: $('shortcut-clear'),
  shortcutHint: $('shortcut-hint'),
  totpEnabled: $('totp-enabled'),
  dataPassword: $('data-password'),
  ttlWrap: $('ttl-wrap'),
  ttlDays: $('ttl-days'),
  lockNow: $('lock-now'),
  dataDir: $('data-dir'),
  dataDirPath: $('data-dir-path'),
  appVersion: $('app-version'),
  checkUpdate: $('check-update'),
  updateStatus: $('update-status'),
  lockOverlay: $('lock'),
  lockForm: $('lock-form'),
  lockHint: $('lock-hint'),
  totpField: $('totp-field'),
  lockTotp: $('lock-totp'),
  passwordField: $('password-field'),
  lockPassword: $('lock-password'),
  lockError: $('lock-error'),
  totpModal: $('totp-modal'),
  totpQr: $('totp-qr'),
  totpSecret: $('totp-secret'),
  copyTotpSecret: $('copy-totp-secret'),
  totpCodeInput: $('totp-code-input'),
  totpError: $('totp-error'),
  totpCancel: $('totp-cancel'),
  totpConfirm: $('totp-confirm'),
  modal: $('modal'),
  modalTitle: $('modal-title'),
  modalText: $('modal-text'),
  modalInput: $('modal-input'),
  modalInput2: $('modal-input2'),
  modalSelect: $('modal-select'),
  modalError: $('modal-error'),
  modalCancel: $('modal-cancel'),
  modalOk: $('modal-ok'),
  entryModal: $('entry-modal'),
  entryModalTitle: $('entry-modal-title'),
  entryTitle: $('entry-title'),
  entryBody: $('entry-body'),
  entryError: $('entry-error'),
  entryCancel: $('entry-cancel'),
  entrySave: $('entry-save'),
  popover: $('popover'),
  toast: $('toast'),
};

const state = {
  groups: [],
  locked: false,
  needTotp: false,
  needDataPassword: false,
  totpEnabled: false,
  dataPassword: false,
  ttlDays: 7,
  lastActive: 0,
  autostart: false,
  startHidden: false,
  dataDir: '',
  version: '',
  toggleShortcut: '',
  shortcutHint: '',
  debug: false,
  updateChecking: false,
  updatePrompted: false,
  updateAutoChecked: false,
  availableUpdate: null,
  selection: { type: 'all', groupId: null, tagId: null },
  query: '',
  settingsOpen: false,
  settingsHideTimer: null,
  recordingShortcut: false,
  entryEditing: null,
};

let toastTimer = null;
let modalResolve = null;
let modalCountdownTimer = null;
let pendingTotpSecret = '';

function h(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function errorText(error) {
  if (typeof error === 'string') return error;
  if (error && error.message) return error.message;
  return String(error);
}

function toast(message, duration = 1600) {
  elements.toast.textContent = message;
  elements.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    elements.toast.hidden = true;
  }, duration);
}

function findGroup(id) {
  return state.groups.find((group) => group.id === id) || null;
}

function findTag(tagId) {
  for (const group of state.groups) {
    const tag = group.tags.find((item) => item.id === tagId);
    if (tag) return { group, tag };
  }
  return null;
}

function totalEntryCount() {
  return state.groups.reduce(
    (total, group) =>
      total + group.tags.reduce((sum, tag) => sum + tag.entries.length, 0),
    0
  );
}

function groupEntryCount(group) {
  return group.tags.reduce((sum, tag) => sum + tag.entries.length, 0);
}

function normalizeSelection() {
  if (state.selection.type === 'tag') {
    const found = findTag(state.selection.tagId);
    if (found) {
      state.selection.groupId = found.group.id;
      return;
    }
    if (findGroup(state.selection.groupId)) {
      state.selection = {
        type: 'group',
        groupId: state.selection.groupId,
        tagId: null,
      };
      return;
    }
    state.selection = { type: 'all', groupId: null, tagId: null };
    return;
  }

  if (state.selection.type === 'group' && !findGroup(state.selection.groupId)) {
    state.selection = { type: 'all', groupId: null, tagId: null };
  }
}

function applySnapshot(snapshot) {
  state.groups = Array.isArray(snapshot.groups) ? snapshot.groups : [];
  state.locked = Boolean(snapshot.locked);
  state.needTotp = Boolean(snapshot.need_totp);
  state.needDataPassword = Boolean(snapshot.need_data_password);
  state.totpEnabled = Boolean(snapshot.totp_enabled);
  state.dataPassword = Boolean(snapshot.data_password);
  state.ttlDays = Number(snapshot.ttl_days) || 7;
  state.lastActive = Number(snapshot.last_active) || 0;
  state.autostart = Boolean(snapshot.autostart);
  state.startHidden = Boolean(snapshot.start_hidden);
  state.dataDir = snapshot.data_dir || '';
  state.version = snapshot.version || '';
  state.toggleShortcut = snapshot.toggle_shortcut || '';
  state.shortcutHint = snapshot.shortcut_hint || '';
  state.debug = Boolean(snapshot.debug);

  if (state.locked) {
    closePopover();
    closeEntryModal();
    closeModal(null);
    closeSettings(true);
    cancelShortcutRecording(true);
  }

  normalizeSelection();
  updateSettingsControls();
  updateLock();
  renderSidebar();
  renderContent();

  if (snapshot.warning) toast(snapshot.warning, 2800);
}

function updateSettingsControls() {
  elements.autostart.checked = state.autostart;
  elements.startHidden.checked = state.startHidden;
  elements.totpEnabled.checked = state.totpEnabled;
  elements.dataPassword.checked = state.dataPassword;
  elements.ttlWrap.hidden = !(state.totpEnabled || state.dataPassword);
  elements.ttlDays.value = String(state.ttlDays);
  elements.lockNow.hidden = !(state.totpEnabled || state.dataPassword);
  elements.dataDirPath.textContent = state.dataDir;
  elements.appVersion.textContent = state.version || '-';

  elements.shortcutRecord.textContent = state.recordingShortcut
    ? '按下组合键…'
    : state.toggleShortcut || '未设置';
  elements.shortcutRecord.classList.toggle('recording', state.recordingShortcut);
  elements.shortcutClear.disabled = state.recordingShortcut || !state.toggleShortcut;
  elements.shortcutHint.textContent = state.shortcutHint;
  elements.shortcutHint.hidden = !state.shortcutHint;
}

function setUpdateStatus(message) {
  elements.updateStatus.textContent = message;
  elements.updateStatus.hidden = !message;
}

async function promptUpdate(info) {
  const notes = (info.notes || '').trim();
  const message = notes
    ? `${notes} 安装完成后程序会自动重启。`
    : '现在下载并安装吗？安装完成后程序会自动重启。';
  const ok = await confirmAction(`发现新版本 ${info.version}`, message, '立即更新');
  if (!ok) {
    setUpdateStatus(`已跳过 ${info.version}`);
    return;
  }
  elements.checkUpdate.disabled = true;
  setUpdateStatus('正在下载并安装更新，请稍候…');
  try {
    await invoke('install_update');
  } catch (error) {
    elements.checkUpdate.disabled = false;
    setUpdateStatus(`更新失败：${errorText(error)}`);
  }
}

async function checkForUpdate({ silent = false } = {}) {
  if (state.updateChecking) return null;
  state.updateChecking = true;
  elements.checkUpdate.disabled = true;
  if (!silent) setUpdateStatus('正在检查更新…');
  try {
    const info = await invoke('check_update');
    if (!info) {
      if (!silent) setUpdateStatus('已经是最新版本');
      return null;
    }
    state.availableUpdate = info;
    setUpdateStatus(`发现新版本 ${info.version}`);
    if (silent && state.updatePrompted) return info;
    if (silent && document.visibilityState === 'hidden') return info;
    state.updatePrompted = true;
    promptUpdate(info);
    return info;
  } catch (error) {
    if (!silent) setUpdateStatus(`检查更新失败：${errorText(error)}`);
    return null;
  } finally {
    state.updateChecking = false;
    elements.checkUpdate.disabled = false;
  }
}

function autoCheckUpdate() {
  if (state.updateAutoChecked || state.locked) return;
  state.updateAutoChecked = true;
  setTimeout(() => {
    if (!state.locked) checkForUpdate({ silent: true });
  }, 4000);
}

function updateLock() {
  elements.lockOverlay.hidden = !state.locked;
  elements.totpField.hidden = !state.needTotp;
  elements.passwordField.hidden = !state.needDataPassword;
  elements.lockHint.textContent = lockHint();
  elements.lockError.textContent = '';
  if (state.locked) {
    elements.lockTotp.value = '';
    elements.lockPassword.value = '';
  }
}

function lockHint() {
  if (!state.locked) return '';
  if (state.needTotp && state.needDataPassword) {
    return '请输入验证器中的 6 位验证码，并输入数据密码解密文件。';
  }
  if (state.needTotp) {
    if (state.lastActive === 0) {
      return '已锁定，请输入验证器中的 6 位验证码。';
    }
    return `超过 ${state.ttlDays} 天未使用，请输入验证器中的 6 位验证码。`;
  }
  if (state.needDataPassword) {
    return '请输入数据密码以解密文件。';
  }
  return '';
}

function focusLockField() {
  const target = !elements.totpField.hidden
    ? elements.lockTotp
    : !elements.passwordField.hidden
      ? elements.lockPassword
      : null;
  if (target) setTimeout(() => target.focus(), 0);
}

function appendHighlighted(parent, text) {
  const query = state.query.trim();
  if (!query) {
    parent.append(document.createTextNode(text));
    return;
  }
  const lowerText = text.toLowerCase();
  const needle = query.toLowerCase();
  let index = 0;
  let found = lowerText.indexOf(needle);
  while (found !== -1) {
    if (found > index) parent.append(document.createTextNode(text.slice(index, found)));
    const mark = h('mark', '', text.slice(found, found + needle.length));
    parent.append(mark);
    index = found + needle.length;
    found = lowerText.indexOf(needle, index);
  }
  if (index < text.length) parent.append(document.createTextNode(text.slice(index)));
}

function renderSidebar() {
  elements.sidebar.replaceChildren();

  const allRow = h('button', 'tree-row all-row');
  allRow.type = 'button';
  allRow.append(h('span', 'tree-dot all-dot'));
  allRow.append(h('span', 'tree-name', '全部'));
  allRow.append(h('span', 'tree-count', String(totalEntryCount())));
  if (state.selection.type === 'all') allRow.classList.add('selected');
  allRow.addEventListener('click', selectAll);
  elements.sidebar.append(allRow);

  if (!state.groups.length) {
    elements.sidebar.append(
      h('div', 'tree-empty', '还没有分组，点右上角 + 新建')
    );
    return;
  }

  for (const group of state.groups) {
    const groupLine = h('div', 'tree-line group-line');
    const groupButton = h('button', 'tree-row group-row');
    groupButton.type = 'button';
    groupButton.append(h('span', 'tree-dot group-dot'));
    const groupName = h('span', 'tree-name');
    appendHighlighted(groupName, group.name);
    groupButton.append(groupName);
    groupButton.append(h('span', 'tree-count', String(groupEntryCount(group))));
    if (state.selection.type === 'group' && state.selection.groupId === group.id) {
      groupButton.classList.add('selected');
    }
    groupButton.addEventListener('click', () => selectGroup(group.id));

    const groupMore = h('button', 'tree-more', '…');
    groupMore.type = 'button';
    groupMore.title = '分组操作';
    groupMore.addEventListener('click', (event) => {
      event.stopPropagation();
      openGroupMenu(groupMore.getBoundingClientRect(), group);
    });

    groupLine.append(groupButton, groupMore);
    elements.sidebar.append(groupLine);

    for (const tag of group.tags) {
      const tagLine = h('div', 'tree-line tag-line');
      const tagButton = h('button', 'tree-row tag-row');
      tagButton.type = 'button';
      tagButton.append(h('span', 'tree-dot tag-dot'));
      const tagName = h('span', 'tree-name');
      appendHighlighted(tagName, tag.name);
      tagButton.append(tagName);
      tagButton.append(h('span', 'tree-count', String(tag.entries.length)));
      if (state.selection.type === 'tag' && state.selection.tagId === tag.id) {
        tagButton.classList.add('selected');
      }
      tagButton.addEventListener('click', () => selectTag(group.id, tag.id));

      const tagStar = h(
        'button',
        `tree-star${tag.starred ? ' starred' : ''}`,
        tag.starred ? '★' : '☆'
      );
      tagStar.type = 'button';
      tagStar.title = tag.starred ? '取消标星' : '标星';
      tagStar.addEventListener('click', (event) => {
        event.stopPropagation();
        toggleTagStar(tag);
      });

      const tagMore = h('button', 'tree-more', '…');
      tagMore.type = 'button';
      tagMore.title = '标签操作';
      tagMore.addEventListener('click', (event) => {
        event.stopPropagation();
        openTagMenu(tagMore.getBoundingClientRect(), group, tag);
      });

      tagLine.append(tagButton, tagStar, tagMore);
      elements.sidebar.append(tagLine);
    }
  }
}

function selectAll() {
  state.selection = { type: 'all', groupId: null, tagId: null };
  closePopover();
  renderSidebar();
  renderContent();
}

function selectGroup(groupId) {
  state.selection = { type: 'group', groupId, tagId: null };
  closePopover();
  renderSidebar();
  renderContent();
}

function selectTag(groupId, tagId) {
  state.selection = { type: 'tag', groupId, tagId };
  closePopover();
  renderSidebar();
  renderContent();
}

function renderBreadcrumb(segments) {
  elements.breadcrumb.replaceChildren();
  segments.forEach((segment, index) => {
    if (index > 0) elements.breadcrumb.append(h('span', 'crumb-sep', '/'));
    const button = h('button', 'crumb', segment.label);
    button.type = 'button';
    button.addEventListener('click', segment.onClick);
    elements.breadcrumb.append(button);
  });
}

function actionButton(label, onClick) {
  const button = h('button', 'action-btn', label);
  button.type = 'button';
  button.addEventListener('click', onClick);
  return button;
}

function renderContentHead() {
  elements.breadcrumb.replaceChildren();

  if (state.selection.type === 'all') {
    renderBreadcrumb([{ label: '全部', onClick: selectAll }]);
    elements.quickAdd.title = '新建分组';
    elements.quickAdd.setAttribute('aria-label', '新建分组');
    return;
  }

  if (state.selection.type === 'group') {
    const group = findGroup(state.selection.groupId);
    if (!group) return;
    renderBreadcrumb([
      { label: '全部', onClick: selectAll },
      { label: group.name, onClick: () => selectGroup(group.id) },
    ]);
    elements.quickAdd.title = '新建标签';
    elements.quickAdd.setAttribute('aria-label', '新建标签');
    return;
  }

  const found = findTag(state.selection.tagId);
  if (!found) return;
  const { group, tag } = found;
  renderBreadcrumb([
    { label: '全部', onClick: selectAll },
    { label: group.name, onClick: () => selectGroup(group.id) },
    { label: tag.name, onClick: () => selectTag(group.id, tag.id) },
  ]);
  elements.quickAdd.title = '新增内容';
  elements.quickAdd.setAttribute('aria-label', '新增内容');
}

function entryMatches(entry, query) {
  const haystack = `${entry.title}\n${(entry.lines || []).join('\n')}`.toLowerCase();
  return haystack.includes(query);
}

function visibleTree() {
  const query = state.query.trim().toLowerCase();
  const result = [];

  for (const group of state.groups) {
    if (
      (state.selection.type === 'group' || state.selection.type === 'tag') &&
      group.id !== state.selection.groupId
    ) {
      continue;
    }

    const groupMatches = Boolean(query) && group.name.toLowerCase().includes(query);
    const tags = [];

    for (const tag of group.tags) {
      if (state.selection.type === 'tag' && tag.id !== state.selection.tagId) {
        continue;
      }
      const tagMatches = Boolean(query) && tag.name.toLowerCase().includes(query);
      let entries = tag.entries;
      if (query && !groupMatches && !tagMatches) {
        entries = entries.filter((entry) => entryMatches(entry, query));
      }

      if (state.selection.type === 'tag') {
        tags.push({ ...tag, entries });
        continue;
      }

      const visible = !query || groupMatches || tagMatches || entries.length > 0;
      if (visible) tags.push({ ...tag, entries });
    }

    if (state.selection.type === 'tag' && tags.length === 0) {
      const selectedTag = group.tags.find((tag) => tag.id === state.selection.tagId);
      if (selectedTag) tags.push({ ...selectedTag, entries: [] });
    }

    if (state.selection.type === 'all' && tags.length === 0) continue;
    if (state.selection.type === 'group' && tags.length === 0 && query && !groupMatches) {
      continue;
    }

    result.push({ ...group, tags });
  }

  return result;
}

function groupDivider(group) {
  const divider = h('div', 'group-divider');
  const name = h('button', 'group-divider-name', group.name);
  name.type = 'button';
  name.addEventListener('click', () => selectGroup(group.id));
  divider.append(name, h('span', 'group-divider-count', `${groupEntryCount(group)} 条`));
  return divider;
}

function tagSection(group, tag) {
  const section = h('section', 'tag-section');
  const head = h('div', 'tag-section-head');

  const link = h('button', 'tag-link');
  link.type = 'button';
  link.append(h('span', 'tag-hash', '#'));
  const name = h('span', 'tag-name');
  appendHighlighted(name, tag.name);
  link.append(name);
  if (tag.starred) {
    const star = h('span', 'tag-star', '★');
    star.title = '已标星';
    link.append(star);
  }
  link.append(h('span', 'tag-count', String(tag.entries.length)));
  link.addEventListener('click', () => selectTag(group.id, tag.id));

  const add = h('button', 'mini-add', '+');
  add.type = 'button';
  add.title = '新增内容';
  add.addEventListener('click', (event) => {
    event.stopPropagation();
    openEntryModal({ tagId: tag.id });
  });

  head.append(link, add);
  section.append(head);

  const list = h('div', 'entry-list');
  if (!tag.entries.length) {
    list.append(
      h(
        'div',
        'section-empty',
        state.query.trim() ? '没有匹配的备忘' : '暂无内容'
      )
    );
  } else {
    for (const entry of tag.entries) {
      list.append(renderEntryCard(group, tag, entry));
    }
  }
  section.append(list);
  return section;
}

function renderContent() {
  renderContentHead();
  elements.cards.replaceChildren();

  if (state.locked) return;

  if (!state.groups.length) {
    const empty = h('div', 'empty-state');
    empty.append(h('div', 'empty-title', '先建一个分组'));
    empty.append(h('p', '', '分组里可以放多个标签，标签下面是具体内容。'));
    const button = h('button', 'action-btn', '+ 新建分组');
    button.type = 'button';
    button.addEventListener('click', createGroup);
    empty.append(button);
    elements.cards.append(empty);
    return;
  }

  const tree = visibleTree();
  let renderedEntries = 0;

  for (const group of tree) {
    if (state.selection.type === 'all') {
      elements.cards.append(groupDivider(group));
    }

    for (const tag of group.tags) {
      if (state.selection.type === 'tag') {
        if (!tag.entries.length) {
          elements.cards.append(
            h(
              'div',
              'empty-state compact',
              state.query.trim() ? '没有匹配的备忘' : '这个标签还没有内容，点「新增内容」'
            )
          );
        } else {
          for (const entry of tag.entries) {
            elements.cards.append(renderEntryCard(group, tag, entry));
            renderedEntries += 1;
          }
        }
      } else {
        elements.cards.append(tagSection(group, tag));
        renderedEntries += tag.entries.length;
      }
    }

    if (state.selection.type === 'group' && !group.tags.length) {
      elements.cards.append(
        h('div', 'empty-state compact', '这个分组还没有标签，点右上角「新建标签」')
      );
    }
  }

  if (renderedEntries === 0 && state.groups.length && !elements.cards.children.length) {
    elements.cards.append(
      h(
        'div',
        'empty-state',
        state.query.trim() ? '没有匹配的备忘' : '还没有内容'
      )
    );
  }

  decorateCards();
}

function decorateCards() {
  for (const card of elements.cards.querySelectorAll('.entry-card')) {
    const lines = card.querySelector('.entry-lines');
    if (!lines) {
      card.classList.add('short');
      continue;
    }
    if (lines.scrollHeight <= lines.clientHeight + 1) card.classList.add('short');
  }

  if (!state.query.trim()) return;

  // 搜索时展开所有命中的卡片，并把第一处命中滚动到可视范围内。
  const marks = elements.cards.querySelectorAll('mark');
  const matchedCards = new Set();
  for (const mark of marks) {
    const card = mark.closest('.entry-card');
    if (card) matchedCards.add(card);
  }
  for (const card of matchedCards) card.classList.add('expanded');

  if (marks.length) {
    requestAnimationFrame(() => {
      if (marks[0].isConnected) {
        marks[0].scrollIntoView({ block: 'center', inline: 'nearest' });
      }
    });
  }
}

const FENCE_RE = /^\s*```/;

function parseBlockLine(line) {
  const heading = line.match(/^\s{0,3}(#{1,6})\s+(.*)$/);
  if (heading) {
    return {
      kind: 'heading',
      level: heading[1].length,
      text: heading[2].replace(/\s*#+\s*$/, ''),
    };
  }
  if (/^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/.test(line)) return { kind: 'hr' };
  const quote = line.match(/^\s*>\s?(.*)$/);
  if (quote) return { kind: 'quote', text: quote[1] };
  const task = line.match(/^\s*[-*+]\s+\[([ xX])\]\s+(.*)$/);
  if (task) {
    return { kind: 'task', checked: task[1].toLowerCase() === 'x', text: task[2] };
  }
  const bullet = line.match(/^\s*[-*+]\s+(.*)$/);
  if (bullet) return { kind: 'ul', text: bullet[1] };
  const ordered = line.match(/^\s*(\d{1,3})[.)]\s+(.*)$/);
  if (ordered) return { kind: 'ol', marker: `${ordered[1]}.`, text: ordered[2] };
  return { kind: 'text', text: line };
}

function parseBody(lines) {
  const blocks = [];
  let fence = null;

  for (const raw of lines) {
    const line = raw.replace(/\r$/, '');
    if (fence) {
      if (FENCE_RE.test(line)) {
        blocks.push({ kind: 'code', text: fence.join('\n') });
        fence = null;
      } else {
        fence.push(line);
      }
      continue;
    }
    if (FENCE_RE.test(line)) {
      fence = [];
      continue;
    }
    if (!line.trim()) continue;
    blocks.push(parseBlockLine(line));
  }

  if (fence) blocks.push({ kind: 'code', text: fence.join('\n') });
  return blocks;
}

function mdPlain(text) {
  return text
    .replace(/!\[([^\]]*)\]\(([^)\s]*)\)/g, (all, alt, url) => alt || url)
    .replace(/\[([^\]]*)\]\(([^)\s]*)\)/g, (all, label, url) => label || url)
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/~~([^~]+)~~/g, '$1')
    .replace(/(^|[^\w*])\*([^*\s][^*]*)\*/g, '$1$2')
    .replace(/(^|[^\w])_([^_\s][^_]*)_(?![\w])/g, '$1$2')
    .trim();
}

function blockPlainText(block) {
  if (block.kind === 'code') return block.text;
  if (block.kind === 'hr') return '';
  return mdPlain(block.text || '');
}

function linePlainText(text) {
  if (!text.trim() || FENCE_RE.test(text)) return '';
  return blockPlainText(parseBlockLine(text));
}

function entryPlainText(entry) {
  return parseBody(entry.lines || [])
    .map(blockPlainText)
    .filter((value) => value.trim())
    .join('\n');
}

function isWebUrl(url) {
  return /^https?:\/\//i.test(url);
}

function openExternal(url) {
  invoke('open_url', { url }).catch((error) => toast(errorText(error), 2600));
}

function appendLink(container, url, label) {
  const target = url.replace(/^<(.*)>$/, '$1');
  if (!isWebUrl(target)) {
    appendHighlighted(container, label);
    return;
  }
  const link = h('span', 'md-link');
  link.setAttribute('role', 'link');
  link.tabIndex = 0;
  link.title = `${target}（Ctrl + 单击打开）`;
  renderInline(link, label, false);
  link.addEventListener('click', (event) => {
    if (!(event.ctrlKey || event.metaKey)) return;
    event.preventDefault();
    event.stopPropagation();
    openExternal(target);
  });
  link.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      event.stopPropagation();
      openExternal(target);
    }
  });
  container.append(link);
}

function renderInline(container, text, allowLinks = true) {
  const pattern =
    /`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|~~([^~]+)~~|!\[([^\]]*)\]\(([^)\s]+)\)|\[([^\]]*)\]\(([^)\s]+)\)|(https?:\/\/[A-Za-z0-9\-._~:/?#@!$&'()*+,;=%\[\]]+)|\*([^*\s][^*]*)\*|_([^_\s][^_]*)_/g;
  let last = 0;
  let match = pattern.exec(text);

  while (match) {
    if (match.index > last) appendHighlighted(container, text.slice(last, match.index));

    if (match[1] !== undefined) {
      const code = h('code', 'inline-code');
      appendHighlighted(code, match[1]);
      container.append(code);
    } else if (match[2] !== undefined || match[3] !== undefined) {
      const strong = h('strong');
      renderInline(strong, match[2] !== undefined ? match[2] : match[3], allowLinks);
      container.append(strong);
    } else if (match[4] !== undefined) {
      const del = h('del');
      renderInline(del, match[4], allowLinks);
      container.append(del);
    } else if (match[5] !== undefined) {
      if (allowLinks) appendLink(container, match[6], match[5] || match[6]);
      else appendHighlighted(container, match[5] || match[6]);
    } else if (match[7] !== undefined) {
      if (allowLinks) appendLink(container, match[8], match[7] || match[8]);
      else appendHighlighted(container, match[7] || match[8]);
    } else if (match[9] !== undefined) {
      let url = match[9];
      const trailing = url.match(/[.,;:!?)\]}。，；：！？]+$/);
      let tail = '';
      if (trailing) {
        tail = trailing[0];
        url = url.slice(0, -tail.length);
      }
      if (allowLinks && url) appendLink(container, url, url);
      else appendHighlighted(container, url || match[9]);
      if (tail) appendHighlighted(container, tail);
    } else if (match[10] !== undefined) {
      const em = h('em');
      renderInline(em, match[10], allowLinks);
      container.append(em);
    } else if (match[11] !== undefined) {
      // 避免把 snake_case 中的下划线当成斜体。
      const before = match.index > 0 ? text[match.index - 1] : '';
      const after = text[match.index + match[0].length] || '';
      if ((before && /[\w]/.test(before)) || (after && /[\w]/.test(after))) {
        appendHighlighted(container, match[0]);
      } else {
        const em = h('em');
        renderInline(em, match[11], allowLinks);
        container.append(em);
      }
    }

    last = match.index + match[0].length;
    match = pattern.exec(text);
  }

  if (last < text.length) appendHighlighted(container, text.slice(last));
}

function copyRow(block) {
  const row = h(
    'div',
    block.kind === 'heading' ? `line-button md-heading md-h${block.level}` : 'line-button'
  );
  row.setAttribute('role', 'button');
  row.tabIndex = 0;

  if (block.kind === 'quote') row.classList.add('md-quote');
  if (block.kind === 'ul' || block.kind === 'ol') {
    row.classList.add('md-li');
    row.append(h('span', 'md-bullet', block.kind === 'ul' ? '•' : block.marker));
  } else if (block.kind === 'task') {
    row.classList.add('md-li', 'md-task');
    if (block.checked) row.classList.add('checked');
    row.append(h('span', 'md-check', block.checked ? '✓' : ''));
  }

  const label = h('span', 'line-text');
  renderInline(label, block.text);
  row.append(label);

  const copy = () => copyText(blockPlainText(block));
  row.title = '点击复制';
  row.addEventListener('click', copy);
  row.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      copy();
    }
  });
  return row;
}

function codeRow(block) {
  const row = h('div', 'code-button');
  row.setAttribute('role', 'button');
  row.tabIndex = 0;
  row.title = '点击复制代码';
  const label = h('span', 'code-text');
  appendHighlighted(label, block.text);
  row.append(label);
  row.addEventListener('click', () => copyText(block.text));
  row.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      copyText(block.text);
    }
  });
  return row;
}

function entryAction(label, title, danger, onClick) {
  const button = h('button', `entry-action${danger ? ' danger' : ''}`, label);
  button.type = 'button';
  button.title = title;
  button.addEventListener('click', (event) => {
    event.stopPropagation();
    onClick();
  });
  return button;
}

function renderEntryCard(group, tag, entry) {
  const card = h('article', 'entry-card');
  const main = h('div', 'entry-main');

  const title = h('button', 'entry-title');
  title.type = 'button';
  title.title = '复制标题';
  appendHighlighted(title, entry.title);
  title.addEventListener('click', () => copyText(mdPlain(entry.title)));
  main.append(title);

  const blocks = parseBody(entry.lines || []);
  if (!blocks.length) {
    main.append(h('span', 'entry-empty', '（无正文）'));
  } else {
    const lines = h('div', 'entry-lines');
    for (const block of blocks) {
      if (block.kind === 'hr') lines.append(h('div', 'md-hr'));
      else if (block.kind === 'code') lines.append(codeRow(block));
      else lines.append(copyRow(block));
    }
    main.append(lines);
  }

  const actions = h('div', 'entry-actions');
  actions.append(
    entryAction('✎', '编辑这条内容', false, () =>
      openEntryModal({ tagId: tag.id, entry })
    ),
    entryAction('⧉', '复制整条', false, () =>
      copyText(entryPlainText(entry) || entry.title)
    ),
    entryAction('×', '删除这条内容', true, () => deleteEntry(entry))
  );

  card.append(main, actions);
  return card;
}

async function copyText(text) {
  if (!text) {
    toast('没有可复制的内容');
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    toast('已复制');
    return;
  } catch (error) {
    // WebView 剪贴板不可用时退回系统命令。
  }
  try {
    await invoke('copy_text', { text });
    toast('已复制');
  } catch (error) {
    toast(errorText(error), 2400);
  }
}

function copyFirstMatch() {
  const query = state.query.trim().toLowerCase();
  for (const group of visibleTree()) {
    for (const tag of group.tags) {
      for (const entry of tag.entries) {
        if (!query) {
          const text = (entry.lines || [])
            .map(linePlainText)
            .find((item) => item.trim());
          if (text) {
            copyText(text);
            return;
          }
          continue;
        }
        for (const line of entry.lines || []) {
          if (!line.toLowerCase().includes(query)) continue;
          const text = linePlainText(line);
          if (text) {
            copyText(text);
            return;
          }
        }
        if (mdPlain(entry.title).toLowerCase().includes(query)) {
          copyText(mdPlain(entry.title));
          return;
        }
      }
    }
  }
  toast('没有可复制的内容');
}

function openPopover(anchorRect, items) {
  elements.popover.replaceChildren();
  for (const item of items) {
    if (item.separator) {
      elements.popover.append(h('div', 'popover-sep'));
      continue;
    }
    const button = h(
      'button',
      `popover-item${item.danger ? ' danger' : ''}`,
      item.label
    );
    button.type = 'button';
    button.addEventListener('click', () => {
      closePopover();
      item.onClick();
    });
    elements.popover.append(button);
  }

  elements.popover.hidden = false;
  const rect = elements.popover.getBoundingClientRect();
  const left = Math.max(
    8,
    Math.min(anchorRect.right - rect.width, window.innerWidth - rect.width - 8)
  );
  let top = anchorRect.bottom + 6;
  if (top + rect.height > window.innerHeight - 8) {
    top = Math.max(8, anchorRect.top - rect.height - 6);
  }
  elements.popover.style.left = `${left}px`;
  elements.popover.style.top = `${top}px`;

  setTimeout(() => {
    document.addEventListener('click', onDocumentClickForPopover, { once: true });
  }, 0);
}

function onDocumentClickForPopover(event) {
  if (!elements.popover.contains(event.target)) closePopover();
}

function closePopover() {
  elements.popover.hidden = true;
  elements.popover.replaceChildren();
}

function openGroupMenu(anchorRect, group) {
  openPopover(anchorRect, [
    { label: '新建标签', onClick: () => createTag(group.id) },
    { label: '重命名分组', onClick: () => renameGroup(group) },
    { label: '删除分组', onClick: () => deleteGroup(group), danger: true },
  ]);
}

function openTagMenu(anchorRect, group, tag) {
  openPopover(anchorRect, [
    {
      label: tag.starred ? '取消标星' : '标星',
      onClick: () => toggleTagStar(tag),
    },
    { label: '重命名标签', onClick: () => renameTag(group, tag) },
    { label: '移动到分组', onClick: () => moveTag(group, tag) },
    { label: '删除标签', onClick: () => deleteTag(group, tag), danger: true },
  ]);
}

function openModal({
  title,
  text = '',
  input = false,
  confirm = false,
  select = null,
  okLabel = '确定',
  inputType = 'text',
  placeholder = '',
  placeholder2 = '',
  initial = '',
  initial2 = '',
  countdown = 0,
  danger = false,
}) {
  return new Promise((resolve) => {
    modalResolve = resolve;
    elements.modalTitle.textContent = title;
    elements.modalText.textContent = text;
    elements.modalText.hidden = !text;

    elements.modalInput.hidden = !input;
    elements.modalInput.type = inputType;
    elements.modalInput.value = initial;
    elements.modalInput.placeholder = placeholder;

    elements.modalInput2.hidden = !confirm;
    elements.modalInput2.type = inputType;
    elements.modalInput2.value = initial2;
    elements.modalInput2.placeholder = placeholder2;

    elements.modalSelect.hidden = !select;
    elements.modalSelect.replaceChildren();
    if (select) {
      for (const option of select) {
        const node = document.createElement('option');
        node.value = option.value;
        node.textContent = option.label;
        elements.modalSelect.append(node);
      }
    }

    elements.modalError.textContent = '';
    clearInterval(modalCountdownTimer);
    modalCountdownTimer = null;
    elements.modalOk.classList.toggle('danger', danger);
    elements.modalOk.disabled = false;
    elements.modalOk.textContent = okLabel;
    elements.modal.hidden = false;

    if (countdown > 0) {
      let remaining = countdown;
      elements.modalOk.disabled = true;
      elements.modalOk.textContent = `${okLabel}（${remaining}s）`;
      modalCountdownTimer = setInterval(() => {
        remaining -= 1;
        if (remaining <= 0) {
          clearInterval(modalCountdownTimer);
          modalCountdownTimer = null;
          elements.modalOk.disabled = false;
          elements.modalOk.textContent = okLabel;
        } else {
          elements.modalOk.textContent = `${okLabel}（${remaining}s）`;
        }
      }, 1000);
    }

    const focusTarget = input
      ? elements.modalInput
      : select
        ? elements.modalSelect
        : elements.modalOk;
    setTimeout(() => {
      if (focusTarget === elements.modalOk && elements.modalOk.disabled) {
        elements.modalCancel.focus();
        return;
      }
      focusTarget.focus();
    }, 0);
  });
}

function closeModal(value) {
  if (!modalResolve) return;
  const resolve = modalResolve;
  modalResolve = null;
  clearInterval(modalCountdownTimer);
  modalCountdownTimer = null;
  elements.modalOk.disabled = false;
  elements.modal.hidden = true;
  resolve(value);
}

async function askPassword(title, text, confirm) {
  for (;;) {
    const result = await openModal({
      title,
      text,
      input: true,
      confirm,
      okLabel: '确定',
      inputType: 'password',
      placeholder: '数据密码（至少 6 位，建议 12 位以上）',
      placeholder2: '再次输入数据密码',
    });
    if (!result) return null;
    if (result.value.length < 6) {
      toast('数据密码至少 6 位');
      continue;
    }
    if (confirm && result.value !== result.value2) {
      toast('两次输入的密码不一致');
      continue;
    }
    if (isWeakPassword(result.value)) {
      const proceed = await confirmAction(
        '密码强度较弱',
        '建议使用 12 位以上的随机密码或长口令。纯数字和短密码很容易被离线暴力破解，仍要使用这个密码吗？',
        '仍然使用'
      );
      if (!proceed) continue;
    }
    return result.value;
  }
}

function isWeakPassword(password) {
  return password.length < 10 || /^\d+$/.test(password);
}

async function promptName(title, placeholder, initial = '') {
  const result = await openModal({
    title,
    input: true,
    okLabel: '保存',
    placeholder,
    initial,
  });
  if (!result) return null;
  const value = result.value.trim();
  if (!value) {
    toast('名称不能为空');
    return null;
  }
  return value;
}

async function confirmAction(title, text, okLabel) {
  const result = await openModal({ title, text, okLabel });
  return Boolean(result);
}

async function createGroup() {
  const name = await promptName('新建分组', '例如：工作、开发、生活');
  if (!name) return;
  try {
    const snapshot = await invoke('create_group', { name });
    applySnapshot(snapshot);
    const group = state.groups[state.groups.length - 1];
    if (group) selectGroup(group.id);
    toast('已新建分组');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function createTag(groupId) {
  const group = findGroup(groupId);
  if (!group) return;
  const before = group.tags.length;
  const name = await promptName('新建标签', '例如：Git、部署、快捷键');
  if (!name) return;
  try {
    const snapshot = await invoke('create_tag', { groupId, name });
    applySnapshot(snapshot);
    const updated = findGroup(groupId);
    const tag = updated && updated.tags[before];
    if (tag) selectTag(groupId, tag.id);
    toast('已新建标签');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function renameGroup(group) {
  const name = await promptName('重命名分组', '分组名称', group.name);
  if (!name || name === group.name) return;
  try {
    const snapshot = await invoke('rename_group', { id: group.id, name });
    applySnapshot(snapshot);
    toast('已重命名分组');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function renameTag(group, tag) {
  const name = await promptName('重命名标签', '标签名称', tag.name);
  if (!name || name === tag.name) return;
  try {
    const snapshot = await invoke('rename_tag', { id: tag.id, name });
    applySnapshot(snapshot);
    toast('已重命名标签');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function toggleTagStar(tag) {
  const starred = !tag.starred;
  try {
    const snapshot = await invoke('set_tag_star', { id: tag.id, starred });
    applySnapshot(snapshot);
    toast(starred ? '已标星' : '已取消标星');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function deleteGroup(group) {
  const ok = await openModal({
    title: `删除分组「${group.name}」？`,
    text: '分组里还有标签时不能删除，请先把标签移走或删除。删除后无法恢复，确认按钮将在 5 秒倒计时结束后可用。',
    okLabel: '确认删除',
    countdown: 5,
    danger: true,
  });
  if (!ok) return;
  try {
    const snapshot = await invoke('delete_group', { id: group.id });
    applySnapshot(snapshot);
    selectAll();
    toast('已删除分组');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function deleteTag(group, tag) {
  if (tag.starred) {
    const ok = await openModal({
      title: `删除标星标签「${tag.name}」？`,
      text: `这是已标星的标签，标签下的 ${tag.entries.length} 条内容会一起删除，无法恢复。确认按钮将在 5 秒倒计时结束后可用。`,
      okLabel: '确认删除',
      countdown: 5,
      danger: true,
    });
    if (!ok) return;
  } else {
    const ok = await confirmAction(
      `删除标签「${tag.name}」？`,
      `标签下的 ${tag.entries.length} 条内容会一起删除，无法恢复。`,
      '删除标签'
    );
    if (!ok) return;
  }
  try {
    const snapshot = await invoke('delete_tag', { id: tag.id });
    applySnapshot(snapshot);
    toast('已删除标签');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

async function moveTag(group, tag) {
  const targets = state.groups.filter((item) => item.id !== group.id);
  if (!targets.length) {
    toast('只有一个分组，先新建一个分组');
    return;
  }
  const result = await openModal({
    title: '移动标签',
    text: `把「${tag.name}」移动到：`,
    select: targets.map((item) => ({ value: item.id, label: item.name })),
    okLabel: '移动',
  });
  if (!result) return;
  try {
    const snapshot = await invoke('move_tag', {
      id: tag.id,
      groupId: result.selection,
    });
    applySnapshot(snapshot);
    toast('已移动标签');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

function openEntryModal({ tagId, entry = null }) {
  state.entryEditing = { tagId, entryId: entry ? entry.id : null };
  elements.entryModalTitle.textContent = entry ? '编辑内容' : '新增内容';
  elements.entryTitle.value = entry ? entry.title : '';
  elements.entryBody.value = entry ? (entry.lines || []).join('\n') : '';
  elements.entryError.textContent = '';
  elements.entryModal.hidden = false;
  setTimeout(() => elements.entryTitle.focus(), 0);
}

function closeEntryModal() {
  elements.entryModal.hidden = true;
  elements.entryError.textContent = '';
  state.entryEditing = null;
}

async function saveEntry() {
  if (!state.entryEditing) return;
  const title = elements.entryTitle.value.trim();
  const body = elements.entryBody.value;
  elements.entryError.textContent = '';

  try {
    let snapshot;
    if (state.entryEditing.entryId) {
      snapshot = await invoke('update_entry', {
        id: state.entryEditing.entryId,
        title,
        body,
      });
    } else {
      snapshot = await invoke('create_entry', {
        tagId: state.entryEditing.tagId,
        title,
        body,
      });
    }
    closeEntryModal();
    applySnapshot(snapshot);
    toast('已保存');
  } catch (error) {
    elements.entryError.textContent = errorText(error);
  }
}

async function deleteEntry(entry) {
  const ok = await confirmAction(
    `删除「${entry.title}」？`,
    '这条内容会从标签里移除，无法恢复。',
    '删除'
  );
  if (!ok) return;
  try {
    const snapshot = await invoke('delete_entry', { id: entry.id });
    applySnapshot(snapshot);
    toast('已删除');
  } catch (error) {
    toast(errorText(error), 2600);
  }
}

function openSettings() {
  if (state.locked) return;
  clearTimeout(state.settingsHideTimer);
  state.settingsOpen = true;
  elements.settingsBackdrop.hidden = false;
  elements.settingsDrawer.hidden = false;
  requestAnimationFrame(() => {
    elements.settingsBackdrop.classList.add('open');
    elements.settingsDrawer.classList.add('open');
  });
}

function closeSettings(immediate = false) {
  if (state.recordingShortcut) cancelShortcutRecording(true);
  state.settingsOpen = false;
  elements.settingsBackdrop.classList.remove('open');
  elements.settingsDrawer.classList.remove('open');
  clearTimeout(state.settingsHideTimer);
  const hide = () => {
    elements.settingsBackdrop.hidden = true;
    elements.settingsDrawer.hidden = true;
  };
  if (immediate) {
    hide();
  } else {
    state.settingsHideTimer = setTimeout(hide, 180);
  }
}

function normalizeShortcutKey(event) {
  const code = event.code;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (code === 'Space') return 'Space';
  return null;
}

function shortcutFromEvent(event) {
  const key = normalizeShortcutKey(event);
  if (!key) return null;
  const mods = [];
  if (event.ctrlKey) mods.push('Ctrl');
  if (event.altKey) mods.push('Alt');
  if (event.shiftKey) mods.push('Shift');
  if (event.metaKey) mods.push('Super');
  if (!mods.length && !/^F\d+$/.test(key)) return null;
  return [...mods, key].join('+');
}

async function startShortcutRecording() {
  if (state.recordingShortcut) return;
  state.recordingShortcut = true;
  updateSettingsControls();
  try {
    await invoke('suspend_shortcut');
  } catch (error) {
    toast(errorText(error), 2400);
  }
  toast('按下新的组合键，Esc 取消', 2200);
}

function cancelShortcutRecording(silent = false) {
  if (!state.recordingShortcut) return;
  state.recordingShortcut = false;
  updateSettingsControls();
  invoke('resume_shortcut').catch((error) => {
    if (!silent) toast(errorText(error), 2400);
  });
}

async function finishShortcutRecording(shortcut) {
  state.recordingShortcut = false;
  updateSettingsControls();
  try {
    // 录制前暂停了旧快捷键；先恢复，再让后端安全地切换。
    await invoke('resume_shortcut').catch(() => {});
    const snapshot = await invoke('set_toggle_shortcut', { shortcut });
    applySnapshot(snapshot);
    toast(`快捷键已设为 ${shortcut}`, 2200);
  } catch (error) {
    toast(errorText(error), 2800);
    updateSettingsControls();
  }
}

async function openTotpSetup() {
  try {
    const setup = await invoke('start_totp_setup');
    pendingTotpSecret = setup.secret;
    const qr = setup.qr_base64.startsWith('data:')
      ? setup.qr_base64
      : `data:image/png;base64,${setup.qr_base64}`;
    elements.totpQr.src = qr;
    elements.totpSecret.textContent = setup.secret.replace(/(.{4})/g, '$1 ').trim();
    elements.totpCodeInput.value = '';
    elements.totpError.textContent = '';
    elements.totpModal.hidden = false;
    elements.totpCodeInput.focus();
  } catch (error) {
    elements.totpEnabled.checked = state.totpEnabled;
    toast(errorText(error), 2600);
  }
}

function closeTotpModal(revert) {
  elements.totpModal.hidden = true;
  pendingTotpSecret = '';
  if (revert) elements.totpEnabled.checked = state.totpEnabled;
}

async function confirmTotpSetup() {
  const code = elements.totpCodeInput.value.trim();
  if (!/^\d{6}$/.test(code)) {
    elements.totpError.textContent = '请输入 6 位数字验证码';
    return;
  }
  try {
    const snapshot = await invoke('confirm_totp_setup', {
      secret: pendingTotpSecret,
      code,
    });
    closeTotpModal(false);
    applySnapshot(snapshot);
    toast('应用锁已开启');
  } catch (error) {
    elements.totpError.textContent = errorText(error);
  }
}

async function requestSnapshot(promise, message) {
  try {
    const snapshot = await promise;
    applySnapshot(snapshot);
    if (message) toast(message);
    return snapshot;
  } catch (error) {
    toast(errorText(error), 2600);
    throw error;
  }
}

function bindEvents() {
  elements.search.addEventListener('input', () => {
    state.query = elements.search.value;
    renderSidebar();
    renderContent();
  });
  elements.search.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      copyFirstMatch();
    }
  });

  elements.refresh.addEventListener('click', async () => {
    try {
      const snapshot = await invoke('get_state');
      applySnapshot(snapshot);
      toast('已刷新');
    } catch (error) {
      toast(errorText(error), 2600);
    }
  });

  elements.settingsOpen.addEventListener('click', openSettings);
  elements.quickAdd.addEventListener('click', () => {
    if (state.selection.type === 'tag') {
      const found = findTag(state.selection.tagId);
      if (found) openEntryModal({ tagId: found.tag.id });
      return;
    }
    if (state.selection.type === 'group') {
      createTag(state.selection.groupId);
      return;
    }
    createGroup();
  });
  elements.githubLink.addEventListener('click', () => openExternal(REPO_URL));
  elements.settingsClose.addEventListener('click', () => closeSettings());
  elements.settingsBackdrop.addEventListener('click', () => closeSettings());

  elements.addGroup.addEventListener('click', createGroup);
  elements.dataDir.addEventListener('click', async () => {
    try {
      await invoke('open_data_dir');
    } catch (error) {
      toast(errorText(error), 2400);
    }
  });

  elements.checkUpdate.addEventListener('click', () => checkForUpdate());

  elements.autostart.addEventListener('change', async () => {
    const enabled = elements.autostart.checked;
    try {
      await requestSnapshot(
        invoke('set_autostart', { enabled }),
        enabled ? '已开启开机启动' : '已关闭开机启动'
      );
    } catch (error) {
      elements.autostart.checked = state.autostart;
    }
  });

  elements.startHidden.addEventListener('change', async () => {
    const enabled = elements.startHidden.checked;
    try {
      await requestSnapshot(
        invoke('set_start_hidden', { enabled }),
        enabled ? '开机启动后将隐藏' : '开机启动后将显示'
      );
    } catch (error) {
      elements.startHidden.checked = state.startHidden;
    }
  });

  elements.totpEnabled.addEventListener('change', async () => {
    if (elements.totpEnabled.checked) {
      await openTotpSetup();
      return;
    }
    const confirmed = await confirmAction(
      '关闭应用锁？',
      '关闭后将不再要求验证器验证码，文件加密不受影响。',
      '关闭应用锁'
    );
    if (!confirmed) {
      elements.totpEnabled.checked = true;
      return;
    }
    try {
      await requestSnapshot(invoke('disable_totp'), '已关闭应用锁');
    } catch (error) {
      elements.totpEnabled.checked = state.totpEnabled;
    }
  });

  elements.dataPassword.addEventListener('change', async () => {
    if (elements.dataPassword.checked) {
      const password = await askPassword(
        '设置数据密码',
        '至少 6 位，建议使用 12 位以上的随机密码或长口令。新设备第一次打开需要输入此密码；忘记后无法恢复。6 位纯数字很容易被离线暴力破解。',
        true
      );
      if (!password) {
        elements.dataPassword.checked = false;
        return;
      }
      try {
        await requestSnapshot(
          invoke('set_data_password', { password }),
          '已启用数据密码'
        );
      } catch (error) {
        elements.dataPassword.checked = state.dataPassword;
      }
      return;
    }

    const confirmed = await confirmAction(
      '关闭数据密码？',
      '文件仍会由软件加密，任何设备上的本软件都能直接打开。',
      '关闭数据密码'
    );
    if (!confirmed) {
      elements.dataPassword.checked = true;
      return;
    }
    try {
      await requestSnapshot(
        invoke('disable_data_password'),
        '已关闭数据密码'
      );
    } catch (error) {
      elements.dataPassword.checked = state.dataPassword;
    }
  });

  elements.ttlDays.addEventListener('change', async () => {
    try {
      await requestSnapshot(
        invoke('set_ttl_days', { days: Number(elements.ttlDays.value) }),
        '已更新验证周期'
      );
    } catch (error) {
      elements.ttlDays.value = String(state.ttlDays);
    }
  });

  elements.lockNow.addEventListener('click', async () => {
    try {
      const snapshot = await invoke('lock');
      closeSettings(true);
      applySnapshot(snapshot);
      focusLockField();
      toast('已锁定');
    } catch (error) {
      toast(errorText(error), 2400);
    }
  });

  elements.shortcutRecord.addEventListener('click', () => {
    if (state.recordingShortcut) {
      cancelShortcutRecording();
    } else {
      startShortcutRecording();
    }
  });

  elements.shortcutClear.addEventListener('click', async () => {
    if (state.recordingShortcut) cancelShortcutRecording(true);
    try {
      const snapshot = await invoke('set_toggle_shortcut', { shortcut: '' });
      applySnapshot(snapshot);
      toast('已清除快捷键');
    } catch (error) {
      toast(errorText(error), 2600);
    }
  });

  elements.lockForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    elements.lockError.textContent = '';
    const totpCode = state.needTotp ? elements.lockTotp.value.trim() : null;
    const dataPassword = state.needDataPassword ? elements.lockPassword.value : null;

    if (state.needTotp && !/^\d{6}$/.test(totpCode)) {
      elements.lockError.textContent = '请输入 6 位数字验证码';
      elements.lockTotp.focus();
      return;
    }
    if (state.needDataPassword && !dataPassword) {
      elements.lockError.textContent = '请输入数据密码';
      elements.lockPassword.focus();
      return;
    }

    try {
      const snapshot = await invoke('unlock', { totpCode, dataPassword });
      applySnapshot(snapshot);
      toast('已解锁');
      elements.search.focus();
      autoCheckUpdate();
    } catch (error) {
      elements.lockError.textContent = errorText(error);
      if (state.needTotp) elements.lockTotp.select();
      else if (state.needDataPassword) elements.lockPassword.select();
    }
  });

  elements.copyTotpSecret.addEventListener('click', () =>
    copyText(pendingTotpSecret)
  );
  elements.totpCancel.addEventListener('click', () => closeTotpModal(true));
  elements.totpConfirm.addEventListener('click', confirmTotpSetup);
  elements.totpCodeInput.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      confirmTotpSetup();
    }
  });

  elements.modalCancel.addEventListener('click', () => closeModal(null));
  elements.modalOk.addEventListener('click', () => {
    if (!elements.modalSelect.hidden) {
      closeModal({ selection: elements.modalSelect.value });
      return;
    }
    if (!elements.modalInput.hidden) {
      if (!elements.modalInput.value) {
        elements.modalError.textContent = '请输入内容';
        return;
      }
      closeModal({
        value: elements.modalInput.value,
        value2: elements.modalInput2.value,
      });
      return;
    }
    closeModal({ ok: true });
  });
  elements.modal.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && event.target.tagName !== 'BUTTON') {
      event.preventDefault();
      elements.modalOk.click();
    }
  });

  elements.entryCancel.addEventListener('click', closeEntryModal);
  elements.entrySave.addEventListener('click', saveEntry);
  elements.entryModal.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      saveEntry();
    }
  });

  document.addEventListener('contextmenu', (event) => {
    if (!state.debug) event.preventDefault();
  });

  document.addEventListener('keydown', (event) => {
    if (state.recordingShortcut) {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        cancelShortcutRecording();
        return;
      }
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) {
        toast('请使用字母、数字或 F1-F24，并加修饰键', 2400);
        return;
      }
      finishShortcutRecording(shortcut);
      return;
    }

    if (event.key === 'Escape') {
      if (!elements.totpModal.hidden) {
        closeTotpModal(true);
        return;
      }
      if (!elements.entryModal.hidden) {
        closeEntryModal();
        return;
      }
      if (!elements.modal.hidden) {
        closeModal(null);
        return;
      }
      if (!elements.popover.hidden) {
        closePopover();
        return;
      }
      if (state.settingsOpen) {
        closeSettings();
        return;
      }
      if (!state.locked) {
        invoke('hide_window').catch(() => {});
      }
      return;
    }

    if (!state.debug && (event.ctrlKey || event.metaKey)) {
      const key = event.key.toLowerCase();
      if (key === 's' || key === 'p' || key === 'u') {
        event.preventDefault();
      }
    }
  });
}

async function init() {
  bindEvents();
  try {
    const snapshot = await invoke('get_state');
    applySnapshot(snapshot);
  } catch (error) {
    toast(`加载失败：${errorText(error)}`, 4000);
  }
  try {
    await invoke('frontend_ready');
  } catch (error) {
    // 窗口仍可通过系统标题栏操作。
  }
  if (state.locked) {
    focusLockField();
  } else {
    setTimeout(() => elements.search.focus(), 0);
  }
  autoCheckUpdate();
}

init();
