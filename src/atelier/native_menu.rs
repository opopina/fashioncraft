//! The studio's menus in macOS's own menu bar, at the top of the screen beside the clock, the
//! way Mac apps show them, instead of inside the window. The bar is built from the same
//! [`MENUS`] as the in-window bar, which Windows and Linux keep (and every system during a
//! scripted demo or a smoke test, which click the in-window menus).
//!
//! The menus show each command's keys but never perform them: they decline every key
//! equivalent, so each key press reaches the window and egui handles shortcuts exactly as it
//! does without this bar. AppKit would otherwise take a key away from a text field (a letter
//! tool key, Cmd+C, Cmd+Z), even for a disabled item. Only the application menu (Settings,
//! Hide, Quit) performs its own keys, which egui has no use for.
//!
//! [`plan`] lays the bar out without AppKit, so it is checked on every system; the AppKit part
//! (macOS only) turns the plan into menus and reports which items were chosen.
// Outside macOS only the tests use the plan, so the build alone sees it as dead code.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
use super::commands::{self, CommandId};
use super::i18n::{self, Language};
use super::keymap::{self, KeymapName};
use super::menus::{Custom, Item, MENUS};
use super::{stage_settings, workspace_profiles};
use eframe::egui;

/// What a menu item does when chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Run(CommandId),
    ToolRail,
    Presentation,
    FilesAndFormats,
    Profiles,
    Keymap(KeymapName),
    Language(Language),
    OpenUrl(&'static str),
    Quit,
}

/// How an item looks this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct State {
    pub enabled: bool,
    pub checked: bool,
    /// Why the item is disabled, shown when the pointer rests on it.
    pub reason: Option<&'static str>,
}

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Node {
    Item {
        title: String,
        action: Action,
        /// The keys egui runs it with, written like "Shift+Cmd+Z".
        keys: Option<String>,
        tip: Option<String>,
    },
    Sub {
        title: String,
        items: Vec<Node>,
    },
    /// A line of text that cannot be chosen.
    Note(String),
    Separator,
}

/// A menu of the bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Menu {
    /// Its English title, which says which menu macOS should treat as the Window and Help menu.
    pub english: &'static str,
    pub title: String,
    pub items: Vec<Node>,
}

/// The FreeSewing designs File › Free patterns links to, as the in-window menu lists them.
const FREE_PATTERNS: [(&str, &str); 3] = [
    ("Teagan · T-shirt", "https://freesewing.eu/designs/teagan/"),
    (
        "Titan · trouser block",
        "https://freesewing.eu/designs/titan/",
    ),
    ("Jett · bomber", "https://freesewing.eu/designs/jett/"),
];
const FREESEWING_TERMS: &str = "https://freesewing.eu/docs/about/faq/commercial-use/";

/// The menus after the application menu, left to right, in `language` with `keymap`'s keys.
pub(super) fn plan(language: Language, keymap: KeymapName) -> Vec<Menu> {
    MENUS
        .iter()
        .map(|(english, items)| Menu {
            english,
            title: i18n::tr(language, english).to_owned(),
            items: nodes(items, language, keymap),
        })
        .collect()
}

fn nodes(items: &[Item], language: Language, keymap: KeymapName) -> Vec<Node> {
    let tr = |text: &'static str| i18n::tr(language, text).to_owned();
    let item = |title: String, action| Node::Item {
        title,
        action,
        keys: None,
        tip: None,
    };
    let mut out = vec![];
    for entry in items {
        match *entry {
            Item::Run(label, id) => out.push(Node::Item {
                title: tr(label),
                action: Action::Run(id),
                keys: keymap::label(keymap, id).or_else(|| id.descriptor().shortcut.map(String::from)),
                tip: None,
            }),
            Item::Sub(label, inner) => out.push(Node::Sub {
                title: tr(label),
                items: nodes(inner, language, keymap),
            }),
            Item::Separator => out.push(Node::Separator),
            Item::Custom(custom) => out.push(match custom {
                Custom::OnlinePatterns => {
                    let tip = Some(tr("Open the website to generate a pattern from your measurements. FreeSewing / SVG import is planned; these links do not add pieces to the scene."));
                    let mut links = vec![Node::Note(tr("FreeSewing · parametric patterns"))];
                    links.extend(FREE_PATTERNS.iter().map(|&(title, url)| Node::Item {
                        title: title.to_owned(),
                        action: Action::OpenUrl(url),
                        keys: None,
                        tip: tip.clone(),
                    }));
                    links.push(Node::Separator);
                    links.push(item("FreeSewing terms of use".into(), Action::OpenUrl(FREESEWING_TERMS)));
                    Node::Sub {
                        title: tr("Free patterns · Web"),
                        items: links,
                    }
                }
                Custom::ExportInformation => item(tr("Files & formats…"), Action::FilesAndFormats),
                Custom::ToolRail => item(tr("Tool rail"), Action::ToolRail),
                Custom::Presentation => item(tr("Presentation…"), Action::Presentation),
                Custom::Profiles => item(tr("Workspace profiles…"), Action::Profiles),
                Custom::Coverage => {
                    let (done, all) = super::menus::coverage();
                    Node::Note(i18n::coverage_line(language, done, all))
                }
                Custom::Keymap => Node::Sub {
                    title: tr("Keyboard map"),
                    items: KeymapName::ALL
                        .into_iter()
                        .map(|map| item(tr(map.label()), Action::Keymap(map)))
                        .collect(),
                },
                Custom::Language => Node::Sub {
                    title: tr("Language"),
                    items: Language::ALL
                        .into_iter()
                        .map(|choice| item(choice.name().to_owned(), Action::Language(choice)))
                        .collect(),
                },
            }),
        }
    }
    out
}

/// Modifier keys of a key equivalent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Modifiers {
    pub command: bool,
    pub shift: bool,
    pub option: bool,
}

/// `keys` ("Shift+Cmd+Z", "H", "Alt+6", "Space") as a macOS key equivalent: the character in
/// lower case (an upper-case letter would mean Shift to AppKit) and its modifiers. None for a
/// key the menu cannot show.
pub(super) fn key_equivalent(keys: &str) -> Option<(String, Modifiers)> {
    let mut modifiers = Modifiers::default();
    let mut parts: Vec<&str> = keys.split('+').collect();
    let key = parts.pop()?;
    for part in parts {
        match part {
            "Cmd" => modifiers.command = true,
            "Shift" => modifiers.shift = true,
            "Alt" => modifiers.option = true,
            _ => return None,
        }
    }
    let character = match key {
        "Space" => " ".to_owned(),
        _ if key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) => {
            key.to_ascii_lowercase()
        }
        _ => return None,
    };
    Some((character, modifiers))
}

/// Every action of `menus`, in order.
#[cfg(test)]
fn actions(menus: &[Menu]) -> Vec<Action> {
    fn walk(items: &[Node], out: &mut Vec<Action>) {
        for node in items {
            match node {
                Node::Item { action, .. } => out.push(*action),
                Node::Sub { items, .. } => walk(items, out),
                Node::Note(_) | Node::Separator => {}
            }
        }
    }
    let mut out = vec![];
    for menu in menus {
        walk(&menu.items, &mut out);
    }
    out
}

/// Windows the macOS menu bar opens for entries the in-window menus show inline.
#[derive(Default)]
pub(super) struct MenuWindows {
    /// Window › Presentation: the lighting controls.
    presentation: bool,
    /// File › Files & formats.
    files: bool,
}

impl super::Atelier {
    /// Runs the items chosen in the macOS menu bar since the last frame. While the palette, the
    /// profiles or a modal dialog holds the interface only Quit goes through, as the in-window
    /// menus are disabled then too.
    pub(super) fn run_native_menu(&mut self, ctx: &egui::Context, palette_blocked: bool) {
        let chosen = match &mut self.native_menu {
            Some(menu) => menu.take_chosen(),
            None => return,
        };
        let blocked = palette_blocked || ctx.memory(|m| m.top_modal_layer().is_some());
        for action in chosen {
            if !blocked || action == Action::Quit {
                self.native_menu_action(action, ctx);
            }
        }
    }

    fn native_menu_action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Run(id) => match commands::availability(id, self.command_context()) {
                Ok(()) => self.invoke_command(id, ctx),
                Err(reason) => self.status = reason.message().into(),
            },
            Action::ToolRail => self.tools_visible = !self.tools_visible,
            Action::Presentation => self.menu_windows.presentation = true,
            Action::FilesAndFormats => self.menu_windows.files = true,
            Action::Profiles => self
                .profiles
                .begin(&self.output.join(workspace_profiles::FILE_NAME)),
            Action::Keymap(map) => {
                self.prefs.keymap = map;
                self.prefs_dirty = true;
            }
            Action::Language(language) => {
                self.prefs.language = language;
                self.prefs_dirty = true;
            }
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            // The way the window's close button closes it, so unsaved work is asked about.
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// Shows this frame's state in the macOS menu bar (rebuilt after a language or keymap
    /// change): what can run, what is on, and why an item cannot run.
    pub(super) fn sync_native_menu(
        &mut self,
        ctx: &egui::Context,
        palette_blocked: bool,
        can_open_palette: bool,
    ) {
        let Some(mut menu) = self.native_menu.take() else {
            return;
        };
        let open = !palette_blocked && ctx.memory(|m| m.top_modal_layer().is_none());
        let context = self.command_context();
        menu.sync(self.prefs.language, self.prefs.keymap, |action| {
            let plain = |enabled, checked| State {
                enabled,
                checked,
                reason: None,
            };
            match action {
                Action::Run(id) => {
                    let available = commands::availability(id, context);
                    State {
                        enabled: open && available.is_ok(),
                        checked: self.command_checked(id),
                        reason: available.err().map(|reason| reason.message()),
                    }
                }
                Action::ToolRail => plain(open, self.tools_visible),
                Action::Presentation => {
                    plain(open && can_open_palette, self.menu_windows.presentation)
                }
                Action::FilesAndFormats => plain(open, self.menu_windows.files),
                Action::Profiles => plain(open && can_open_palette, false),
                Action::Keymap(map) => plain(open, self.prefs.keymap == map),
                Action::Language(language) => plain(open, self.prefs.language == language),
                Action::OpenUrl(_) => plain(open, false),
                Action::Quit => plain(true, false),
            }
        });
        self.native_menu = Some(menu);
    }

    /// Window › Presentation and File › Files & formats, which the in-window menus show
    /// inline, as small windows.
    pub(super) fn native_menu_windows(&mut self, ctx: &egui::Context, idle: bool) {
        let language = self.prefs.language;
        let mut open = self.menu_windows.presentation;
        egui::Window::new(i18n::tr(language, "Presentation"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                if let Some(action) = stage_settings::controls(ui, idle, &mut self.prefs.viewport) {
                    let path = self.output.join(stage_settings::FILE_NAME);
                    self.status = stage_settings::execute(
                        action,
                        &path,
                        &mut self.camera,
                        &mut self.prefs.viewport,
                    )
                    .unwrap_or_else(|e| format!("Presentation: {e}"));
                }
            });
        self.menu_windows.presentation = open;
        let mut open = self.menu_windows.files;
        egui::Window::new(i18n::tr(language, "Files & formats"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.files_and_formats(ui));
        self.menu_windows.files = open;
    }
}

#[cfg(target_os = "macos")]
pub(super) use appkit::NativeMenu;

/// ⌘V was pressed. The menu sees every ⌘ key before the window does, and egui hears nothing of
/// a paste whose clipboard holds no text (a picture, or files copied in the Finder), so the
/// Assistant learns of it here.
static PASTE_KEY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether ⌘V was pressed since the last call (macOS only; elsewhere always false).
pub(super) fn take_paste_key() -> bool {
    PASTE_KEY.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Watch every key the app receives for ⌘V, before the menu bar, a text field or the window
/// handle it, so a picture or files can be pasted into the Assistant however FashionCraft was
/// started (a .app or the bare binary) and whether or not the menu bar is there. The key goes
/// on as before: text still pastes into every text field. Once, at start; macOS only.
#[cfg(target_os = "macos")]
pub(super) fn watch_paste_key() {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;
    static WATCHING: std::sync::Once = std::sync::Once::new();
    WATCHING.call_once(|| {
        let block = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands the monitor a valid event for the call's duration.
            let e = unsafe { event.as_ref() };
            let flags = e.modifierFlags();
            let others = NSEventModifierFlags::Control | NSEventModifierFlags::Option;
            // V by its character, or by its key on layouts without a Latin V (kVK_ANSI_V).
            let v = e
                .charactersIgnoringModifiers()
                .is_some_and(|c| c.to_string().eq_ignore_ascii_case("v"))
                || e.keyCode() == 9;
            if v && flags.contains(NSEventModifierFlags::Command) && !flags.intersects(others) {
                PASTE_KEY.store(true, std::sync::atomic::Ordering::Relaxed);
                tracing::debug!("⌘V seen");
            }
            event.as_ptr()
        });
        // SAFETY: the handler returns the event it was given, a valid pointer.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
        };
        // It watches for as long as the app runs.
        std::mem::forget(monitor);
    });
}

#[cfg(not(target_os = "macos"))]
pub(super) fn watch_paste_key() {}

/// Where macOS has no menu bar of its own to use, the in-window bar stays.
#[cfg(not(target_os = "macos"))]
pub(super) enum NativeMenu {}

#[cfg(not(target_os = "macos"))]
impl NativeMenu {
    pub(super) fn install(_ctx: &egui::Context) -> Option<Self> {
        None
    }
    pub(super) fn take_chosen(&mut self) -> Vec<Action> {
        match *self {}
    }
    pub(super) fn sync(
        &mut self,
        _language: Language,
        _keymap: KeymapName,
        _state: impl FnMut(Action) -> State,
    ) {
        match *self {}
    }
}

#[cfg(target_os = "macos")]
mod appkit {
    use super::{Action, Node, State, key_equivalent, plan};
    use super::{CommandId, KeymapName, Language, egui, i18n};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEvent,
        NSEventModifierFlags, NSMenu, NSMenuItem, NSWindow,
    };
    use objc2_foundation::NSString;
    use std::cell::RefCell;

    // A menu that shows its items' keys but leaves the keys to the window. Keys with Control or
    // fn, which no FashionCraft command uses, still go to the items macOS adds to the Edit and
    // Window menus (full screen, tiling, emoji).
    define_class!(
        // SAFETY: NSMenu has no subclassing requirements, the override keeps the method's
        // signature, and the class does not implement Drop.
        #[unsafe(super(NSMenu))]
        #[thread_kind = MainThreadOnly]
        #[name = "FashionCraftQuietMenu"]
        struct QuietMenu;

        impl QuietMenu {
            #[unsafe(method(performKeyEquivalent:))]
            fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
                let system = NSEventModifierFlags::Control | NSEventModifierFlags::Function;
                if event.modifierFlags().intersects(system) {
                    // SAFETY: NSMenu's own method, with the event it was given.
                    unsafe { msg_send![super(self), performKeyEquivalent: event] }
                } else {
                    let v = event
                        .charactersIgnoringModifiers()
                        .is_some_and(|c| c.to_string().eq_ignore_ascii_case("v"));
                    if v && event.modifierFlags().contains(NSEventModifierFlags::Command) {
                        super::PASTE_KEY.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    false
                }
            }
        }
    );

    struct TargetIvars {
        chosen: RefCell<Vec<isize>>,
        ctx: egui::Context,
    }

    // Receives the items chosen in the menus and wakes the studio to run them.
    define_class!(
        // SAFETY: NSObject has no subclassing requirements and the class does not implement
        // Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "FashionCraftMenuTarget"]
        #[ivars = TargetIvars]
        struct Target;

        unsafe impl NSObjectProtocol for Target {}

        impl Target {
            #[unsafe(method(choose:))]
            fn choose(&self, sender: Option<&NSMenuItem>) {
                if let Some(item) = sender {
                    self.ivars().chosen.borrow_mut().push(item.tag());
                    self.ivars().ctx.request_repaint();
                }
            }
        }
    );

    impl Target {
        fn new(mtm: MainThreadMarker, ctx: &egui::Context) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(TargetIvars {
                chosen: RefCell::default(),
                ctx: ctx.clone(),
            });
            // SAFETY: NSObject's designated initialiser.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// The studio's menus in the macOS menu bar.
    pub(crate) struct NativeMenu {
        mtm: MainThreadMarker,
        /// Menu items hold their target weakly: this keeps it alive.
        target: Retained<Target>,
        /// The language and keymap the bar was built in; None before the first build.
        built_for: Option<(Language, KeymapName)>,
        /// What each item does and the item, numbered by the item's tag.
        actions: Vec<Action>,
        items: Vec<Retained<NSMenuItem>>,
        /// The state each item shows; None until first set.
        shown: Vec<Option<State>>,
    }

    impl NativeMenu {
        /// The menu bar, unless `FASHIONCRAFT_WINDOW_MENUS=1` keeps the menus in the window.
        /// It fills in on the first [`NativeMenu::sync`].
        pub(crate) fn install(ctx: &egui::Context) -> Option<Self> {
            if std::env::var_os("FASHIONCRAFT_WINDOW_MENUS").is_some_and(|v| v == "1") {
                return None;
            }
            let mtm = MainThreadMarker::new()?;
            // Each window is a process of its own, so windows never merge into tabs: no tab
            // items in the Window menu.
            NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
            Some(Self {
                mtm,
                target: Target::new(mtm, ctx),
                built_for: None,
                actions: vec![],
                items: vec![],
                shown: vec![],
            })
        }

        /// The items chosen since the last call, oldest first.
        pub(crate) fn take_chosen(&mut self) -> Vec<Action> {
            let tags = std::mem::take(&mut *self.target.ivars().chosen.borrow_mut());
            tags.into_iter()
                .filter_map(|tag| usize::try_from(tag).ok())
                .filter_map(|i| self.actions.get(i).copied())
                .collect()
        }

        /// Rebuilds the bar when the language or keymap changed, then shows each item's
        /// `state`, touching AppKit only for items whose state changed.
        pub(crate) fn sync(
            &mut self,
            language: Language,
            keymap: KeymapName,
            mut state: impl FnMut(Action) -> State,
        ) {
            if self.built_for != Some((language, keymap)) {
                self.build(language, keymap);
            }
            for (i, action) in self.actions.iter().enumerate() {
                let now = state(*action);
                if self.shown[i].as_ref() == Some(&now) {
                    continue;
                }
                let item = &self.items[i];
                item.setEnabled(now.enabled);
                item.setState(if now.checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
                if matches!(action, Action::Run(_)) {
                    let tip = now.reason.filter(|_| !now.enabled).map(NSString::from_str);
                    item.setToolTip(tip.as_deref());
                }
                self.shown[i] = Some(now);
            }
        }

        fn build(&mut self, language: Language, keymap: KeymapName) {
            let mtm = self.mtm;
            self.actions.clear();
            self.items.clear();
            // Choices made in the old bar would be read against the new numbering.
            self.target.ivars().chosen.borrow_mut().clear();
            let app = NSApplication::sharedApplication(mtm);
            let bar =
                NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("FashionCraft"));
            bar.addItem(&holder(mtm, &self.app_menu(language)));
            for menu in plan(language, keymap) {
                let submenu = self.menu(&menu.title, &menu.items);
                bar.addItem(&holder(mtm, &submenu));
                match menu.english {
                    "Window" => app.setWindowsMenu(Some(&submenu)),
                    "Help" => app.setHelpMenu(Some(&submenu)),
                    _ => {}
                }
            }
            app.setMainMenu(Some(&bar));
            self.shown = vec![None; self.actions.len()];
            self.built_for = Some((language, keymap));
        }

        /// The application menu (the one macOS names after the app): About, Settings,
        /// Services, Hide and Quit, the way Mac apps have it. Its keys work natively; Quit
        /// closes the window the way its close button does, so unsaved work is asked about.
        fn app_menu(&mut self, language: Language) -> Retained<NSMenu> {
            let mtm = self.mtm;
            let tr = |text: &'static str| NSString::from_str(i18n::tr(language, text));
            let menu =
                NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("FashionCraft"));
            menu.setAutoenablesItems(false);
            let standard = |title: &str, action: Sel, key: &str, mods: NSEventModifierFlags| {
                // SAFETY: the action is one NSApplication answers; a nil target sends it along
                // the responder chain, which ends at the application.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &tr_owned(language, title),
                        Some(action),
                        &NSString::from_str(key),
                    )
                };
                item.setKeyEquivalentModifierMask(mods);
                item
            };
            let none = NSEventModifierFlags::empty();
            let cmd = NSEventModifierFlags::Command;
            menu.addItem(&standard(
                "About FashionCraft",
                sel!(orderFrontStandardAboutPanel:),
                "",
                none,
            ));
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let settings = i18n::tr(language, "Settings…");
            menu.addItem(&self.item(settings, Action::Run(CommandId::OpenPreferences), ","));
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let services = NSMenu::initWithTitle(NSMenu::alloc(mtm), &tr("Services"));
            menu.addItem(&holder(mtm, &services));
            NSApplication::sharedApplication(mtm).setServicesMenu(Some(&services));
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            menu.addItem(&standard("Hide FashionCraft", sel!(hide:), "h", cmd));
            menu.addItem(&standard(
                "Hide Others",
                sel!(hideOtherApplications:),
                "h",
                cmd | NSEventModifierFlags::Option,
            ));
            menu.addItem(&standard(
                "Show All",
                sel!(unhideAllApplications:),
                "",
                none,
            ));
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let quit = i18n::tr(language, "Quit FashionCraft");
            menu.addItem(&self.item(quit, Action::Quit, "q"));
            menu
        }

        /// A menu of `nodes` that leaves its keys to the window.
        fn menu(&mut self, title: &str, nodes: &[Node]) -> Retained<NSMenu> {
            let mtm = self.mtm;
            let this = QuietMenu::alloc(mtm).set_ivars(());
            // SAFETY: NSMenu's designated initialiser.
            let menu: Retained<QuietMenu> =
                unsafe { msg_send![super(this), initWithTitle: &*NSString::from_str(title)] };
            let menu = Retained::into_super(menu);
            menu.setAutoenablesItems(false);
            for node in nodes {
                match node {
                    Node::Item {
                        title,
                        action,
                        keys,
                        tip,
                    } => {
                        let item = self.item(title, *action, "");
                        if let Some((key, mods)) = keys.as_deref().and_then(key_equivalent) {
                            item.setKeyEquivalent(&NSString::from_str(&key));
                            let mut mask = NSEventModifierFlags::empty();
                            mask.set(NSEventModifierFlags::Command, mods.command);
                            mask.set(NSEventModifierFlags::Shift, mods.shift);
                            mask.set(NSEventModifierFlags::Option, mods.option);
                            item.setKeyEquivalentModifierMask(mask);
                        }
                        if let Some(tip) = tip {
                            item.setToolTip(Some(&NSString::from_str(tip)));
                        }
                        menu.addItem(&item);
                    }
                    Node::Sub { title, items } => {
                        let submenu = self.menu(title, items);
                        menu.addItem(&holder(mtm, &submenu));
                    }
                    Node::Note(text) => {
                        let item = NSMenuItem::new(mtm);
                        item.setTitle(&NSString::from_str(text));
                        item.setEnabled(false);
                        menu.addItem(&item);
                    }
                    Node::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                }
            }
            menu
        }

        /// An item that reports `action` when chosen, with `key` (and Command) as its key.
        fn item(&mut self, title: &str, action: Action, key: &str) -> Retained<NSMenuItem> {
            let mtm = self.mtm;
            // SAFETY: `choose:` is a method of the target, which takes the item as sender.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(sel!(choose:)),
                    &NSString::from_str(key),
                )
            };
            let target: &AnyObject = &self.target;
            // SAFETY: the target answers `choose:` and `self` keeps it alive.
            unsafe { item.setTarget(Some(target)) };
            item.setTag(self.items.len() as isize);
            self.actions.push(action);
            self.items.push(item.clone());
            item
        }
    }

    fn tr_owned(language: Language, text: &str) -> Retained<NSString> {
        NSString::from_str(i18n::tr_any(language, text))
    }

    /// The bar item that opens `menu`, titled like it.
    fn holder(mtm: MainThreadMarker, menu: &NSMenu) -> Retained<NSMenuItem> {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&menu.title());
        item.setSubmenu(Some(menu));
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_command_is_in_the_native_bar() {
        fn runs(items: &[Item], out: &mut Vec<CommandId>) {
            for item in items {
                match item {
                    Item::Run(_, id) => out.push(*id),
                    Item::Sub(_, inner) => runs(inner, out),
                    _ => {}
                }
            }
        }
        let mut wanted = vec![];
        for (_, items) in MENUS {
            runs(items, &mut wanted);
        }
        for language in Language::ALL {
            let menus = plan(language, KeymapName::Compatible);
            assert_eq!(menus.len(), MENUS.len());
            let ran: Vec<CommandId> = actions(&menus)
                .into_iter()
                .filter_map(|a| match a {
                    Action::Run(id) => Some(id),
                    _ => None,
                })
                .collect();
            assert_eq!(ran, wanted, "{language:?}");
        }
    }

    #[test]
    fn every_custom_entry_has_a_native_form() {
        let menus = plan(Language::Spanish, KeymapName::Compatible);
        let all = actions(&menus);
        for action in [
            Action::ToolRail,
            Action::Presentation,
            Action::FilesAndFormats,
            Action::Profiles,
            Action::Keymap(KeymapName::Classic),
            Action::Language(Language::English),
            Action::OpenUrl(FREESEWING_TERMS),
        ] {
            assert!(all.contains(&action), "{action:?} missing");
        }
        assert_eq!(menus[0].title, "Archivo");
    }

    #[test]
    fn keys_become_key_equivalents() {
        let cmd = Modifiers {
            command: true,
            ..Default::default()
        };
        assert_eq!(key_equivalent("Cmd+Z"), Some(("z".into(), cmd)));
        assert_eq!(
            key_equivalent("Shift+Cmd+S"),
            Some(("s".into(), Modifiers { shift: true, ..cmd }))
        );
        assert_eq!(
            key_equivalent("H"),
            Some(("h".into(), Modifiers::default()))
        );
        assert_eq!(key_equivalent("Alt+6").map(|k| k.1.option), Some(true));
        assert_eq!(
            key_equivalent("Space"),
            Some((" ".into(), Modifiers::default()))
        );
        assert_eq!(key_equivalent("Ctrl+K"), None);
        assert_eq!(key_equivalent("F12"), None);
    }

    #[test]
    fn every_key_the_menus_show_is_one_the_bar_can_show() {
        for map in KeymapName::ALL {
            for menu in plan(Language::English, map) {
                fn walk(items: &[Node]) {
                    for node in items {
                        match node {
                            Node::Item {
                                keys: Some(keys), ..
                            } => {
                                assert!(key_equivalent(keys).is_some(), "{keys}");
                            }
                            Node::Sub { items, .. } => walk(items),
                            _ => {}
                        }
                    }
                }
                walk(&menu.items);
            }
        }
    }
}
