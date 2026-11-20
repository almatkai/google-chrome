// Chrome dictionary operations corresponding to pinned upstream src/actions.
// Invoked only after an explicit command or action; no background/profile scan.
function main() {
  const chrome = Application('Google Chrome');
  const result = [];
  if (input.kind === 'tabs') {
    for (const win of chrome.windows()) {
      let index = 0;
      for (const tab of win.tabs()) {
        index++;
        let favicon = '';
        if (input.favicon) {
          favicon = tab.execute({javascript: "document.head.querySelector('link[rel~=icon]') ? document.head.querySelector('link[rel~=icon]').href : '';"});
        }
        result.push({kind:'tab',id:String(win.id())+':'+String(tab.id()),tabId:tab.id(),title:tab.title(),url:tab.url(),window:win.id(),index:index,favicon:favicon});
      }
    }
    return result;
  }
  if (input.kind === 'windows') {
    for (const win of chrome.windows()) {
      let url=''; try {url=win.activeTab.url();} catch (_) {}
      result.push({kind:'window',id:String(win.id()),window:win.id(),title:win.title(),url:url});
    }
    return result;
  }
  if (input.kind === 'active-url') {
    try {return chrome.windows[0].activeTab.url();} catch (_) {return '';}
  }
  if (input.kind === 'name-window') {
    chrome.activate();
    const system=Application('System Events');
    system.processes.byName('Google Chrome').menuBars[0].menuBarItems.byName('Window').menus[0].menuItems.byName('Name Window…').click();
    return true;
  }
  if (input.kind === 'new-window' || input.kind === 'new-incognito-window') {
    chrome.Window(input.kind === 'new-incognito-window' ? {mode:'incognito'} : {}).make();
    chrome.activate(); return true;
  }
  if (input.kind === 'new-tab') {
    if (!chrome.windows().length) chrome.Window().make();
    chrome.windows[0].tabs.push(chrome.Tab({url:input.url}));
    chrome.activate(); return true;
  }
  // Identity is captured in server-owned session data. Re-resolve windows by ID
  // and tabs by stable Chrome ID. An index and URL cannot distinguish duplicate
  // tabs after another tab closes or moves. Address effects by ID as well.
  const matches=chrome.windows.whose({id:input.window})();
  if (!matches.length) throw Error('Chrome window no longer exists; refresh');
  const win=matches[0];
  if (input.kind === 'focus-window') {win.index=1;chrome.activate();return true;}
  if (!Number.isSafeInteger(input.tabId)) throw Error('Chrome tab identity missing; refresh');
  const index=win.tabs().findIndex(tab => tab.id() === input.tabId);
  if (index < 0) throw Error('Chrome tab no longer exists; refresh');
  const tab=win.tabs.byId(input.tabId);
  if (tab.url() !== input.url) throw Error('Chrome tab changed; refresh before acting');
  if (input.kind === 'close-tab') tab.close();
  else if (input.kind === 'reload-tab') tab.reload();
  else if (input.kind === 'focus-tab') {win.index=1;win.activeTabIndex=index+1;chrome.activate();}
  else throw Error('Unsupported Chrome action');
  return true;
}
JSON.stringify(main());
