use super::*;

impl ClapPluginHostNode {
    /// Control path: toggle bypass.
    ///
    /// Re-enabling is also the explicit retry for a plugin that failed
    /// mid-stream: it clears the failure latch so the plugin gets another
    /// chance to process (and latches again if it fails again).
    #[allow(dead_code)]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.process_errors.store(0, Ordering::Relaxed);
        }
    }

    /// Whether a plugin is currently instantiated.
    pub fn is_loaded(&self) -> bool {
        #[cfg(feature = "clap-host")]
        {
            self.instance.is_some()
        }
        #[cfg(not(feature = "clap-host"))]
        {
            false
        }
    }

    /// The dynamic library this node was loaded from.
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    /// The plugin's own descriptor, read from the live instance.
    pub fn describe(&self) -> Option<ClapPluginInfo> {
        #[cfg(feature = "clap-host")]
        {
            let inst = self.instance.as_ref()?;
            if inst.plugin.is_null() {
                return None;
            }
            let desc_ptr = unsafe { (*inst.plugin).desc };
            if desc_ptr.is_null() {
                return None;
            }
            Some(plugin_info_from(unsafe { &*desc_ptr }))
        }
        #[cfg(not(feature = "clap-host"))]
        {
            None
        }
    }

    /// Every parameter the plugin exposes, with the value it reports.
    ///
    /// Must run on a non-audio thread: the CLAP `params` extension reserves
    /// `get_info` and `get_value` for the host's main thread, which is the
    /// engine thread that called `load_plugin`.
    pub fn param_infos(&self) -> Vec<ClapParamInfo> {
        #[cfg(feature = "clap-host")]
        {
            let mut out = Vec::new();
            let Some(inst) = self.instance.as_ref() else {
                return out;
            };
            if inst.params_ext.is_null() || inst.plugin.is_null() {
                return out;
            }
            let ext = unsafe { &*inst.params_ext };
            let Some(count) = ext.count else {
                return out;
            };
            let total = unsafe { count(inst.plugin) };

            for index in 0..total {
                let mut raw = clap_param_info {
                    id: 0,
                    flags: 0,
                    cookie: std::ptr::null_mut(),
                    name: [0; CLAP_NAME_SIZE],
                    module: [0; CLAP_PATH_SIZE],
                    min_value: 0.0,
                    max_value: 0.0,
                    default_value: 0.0,
                };
                let have_info = match ext.get_info {
                    Some(get_info) => unsafe { get_info(inst.plugin, index, &mut raw) },
                    None => false,
                };
                if !have_info {
                    continue;
                }

                let mut value = raw.default_value;
                if let Some(get_value) = ext.get_value {
                    let mut reported = 0.0f64;
                    if unsafe { get_value(inst.plugin, raw.id, &mut reported) } {
                        value = reported;
                    }
                }

                // The plugin renders its own value: for an enumeration that is
                // the name of the mode, which a bare number cannot convey.
                let mut value_text = String::new();
                if let Some(to_text) = ext.value_to_text {
                    let mut buffer = [0 as c_char; CLAP_NAME_SIZE];
                    let wrote = unsafe {
                        to_text(
                            inst.plugin,
                            raw.id,
                            value,
                            buffer.as_mut_ptr(),
                            buffer.len() as u32,
                        )
                    };
                    if wrote {
                        value_text = cstr_buf_to_string(&buffer);
                    }
                }

                out.push(ClapParamInfo {
                    id: raw.id,
                    name: cstr_buf_to_string(&raw.name),
                    module: cstr_buf_to_string(&raw.module),
                    min: raw.min_value,
                    max: raw.max_value,
                    default: raw.default_value,
                    value,
                    value_text,
                    is_stepped: raw.flags & CLAP_PARAM_IS_STEPPED != 0,
                    is_periodic: raw.flags & CLAP_PARAM_IS_PERIODIC != 0,
                    is_readonly: raw.flags & CLAP_PARAM_IS_READONLY != 0,
                    is_hidden: raw.flags & CLAP_PARAM_IS_HIDDEN != 0,
                    is_bypass: raw.flags & CLAP_PARAM_IS_BYPASS != 0,
                    is_enum: raw.flags & CLAP_PARAM_IS_ENUM != 0,
                });
            }
            out
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Vec::new()
        }
    }

    /// The plugin's own text for a value, for enumeration and unit labels.
    pub fn param_text(&self, id: u32, value: f64) -> Option<String> {
        #[cfg(feature = "clap-host")]
        {
            let inst = self.instance.as_ref()?;
            if inst.params_ext.is_null() || inst.plugin.is_null() {
                return None;
            }
            let ext = unsafe { &*inst.params_ext };
            let to_text = ext.value_to_text?;
            let mut buffer = [0 as c_char; CLAP_NAME_SIZE];
            let ok = unsafe {
                to_text(
                    inst.plugin,
                    id,
                    value,
                    buffer.as_mut_ptr(),
                    CLAP_NAME_SIZE as u32,
                )
            };
            if ok {
                return Some(cstr_buf_to_string(&buffer));
            }
            None
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = (id, value);
            None
        }
    }

    /// The window APIs this host can offer a floating CLAP editor, in
    /// preference order. Wayland first when the session is Wayland; X11
    /// otherwise (XWayland covers X11-only plugins on most Wayland desktops).
    #[cfg(feature = "clap-host")]
    fn gui_api_candidates() -> Vec<&'static CStr> {
        if cfg!(target_os = "macos") {
            vec![CLAP_WINDOW_API_COCOA]
        } else if cfg!(target_os = "windows") {
            vec![CLAP_WINDOW_API_WIN32]
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            vec![CLAP_WINDOW_API_WAYLAND, CLAP_WINDOW_API_X11]
        } else {
            vec![CLAP_WINDOW_API_X11, CLAP_WINDOW_API_WAYLAND]
        }
    }

    /// The first floating-editor API the plugin accepts, preferring its own
    /// choice when that choice is one this host can provide.
    #[cfg(feature = "clap-host")]
    fn choose_gui_api(&self, inst: &ClapInstance) -> Option<&'static CStr> {
        if inst.gui_ext.is_null() || inst.plugin.is_null() {
            return None;
        }
        let ext = unsafe { &*inst.gui_ext };
        let supported = |api: &'static CStr| {
            ext.is_api_supported.is_some_and(|is_supported| unsafe {
                is_supported(inst.plugin, api.as_ptr(), true)
            })
        };
        let candidates = Self::gui_api_candidates();
        if let Some(preferred) = ext.get_preferred_api {
            let mut api: *const c_char = std::ptr::null();
            let mut floating = false;
            if unsafe { preferred(inst.plugin, &mut api, &mut floating) }
                && floating
                && !api.is_null()
            {
                let chosen = unsafe { CStr::from_ptr(api) };
                if let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| chosen == *candidate && supported(candidate))
                {
                    return Some(*candidate);
                }
            }
        }
        candidates.into_iter().find(|api| supported(api))
    }

    /// What the rack needs to decide whether to show an editor button.
    pub fn gui_info(&self) -> ClapGuiInfo {
        #[cfg(feature = "clap-host")]
        {
            let Some(inst) = self.instance.as_ref() else {
                return ClapGuiInfo::default();
            };
            if inst.gui_ext.is_null() {
                return ClapGuiInfo::default();
            }
            let available = self.choose_gui_api(inst).is_some();
            let destroyed = self.gui_signals.destroyed.load(Ordering::Relaxed);
            let open =
                inst.gui_created && !destroyed && self.gui_signals.open.load(Ordering::Relaxed);
            ClapGuiInfo {
                available,
                open,
                api: inst.gui_api.clone(),
                width: self.gui_signals.width.load(Ordering::Relaxed),
                height: self.gui_signals.height.load(Ordering::Relaxed),
                error: inst.gui_error.clone(),
            }
        }
        #[cfg(not(feature = "clap-host"))]
        {
            ClapGuiInfo::default()
        }
    }

    /// Create (or re-show) the plugin's floating native editor.
    ///
    /// Floating only, deliberately: `create(api, true)` lets the plugin own a
    /// top-level window, so the host never has to punch a hole in egui or
    /// hand a plugin an X11/Wayland child it does not understand. A plugin
    /// that can only render embedded is reported as having no editor rather
    /// than being silently half-hosted.
    pub fn open_gui(&mut self) -> Result<(), String> {
        #[cfg(feature = "clap-host")]
        {
            if self.instance.is_none() {
                return Err("No CLAP plugin is loaded".to_string());
            }
            let Some(api) = self.choose_gui_api(self.instance.as_ref().expect("checked above"))
            else {
                let message = "The plugin does not provide a floating GUI for this display server"
                    .to_string();
                if let Some(inst) = self.instance.as_mut() {
                    inst.gui_error = Some(message.clone());
                }
                return Err(message);
            };

            let inst = self.instance.as_mut().expect("checked above");
            if self.gui_signals.destroyed.swap(false, Ordering::Relaxed) {
                // The plugin destroyed its own window and told the host via
                // `clap.gui.closed`: the handle is gone, so do not call
                // `destroy` on it again.
                inst.gui_created = false;
                inst.gui_api = None;
            }
            let ext = unsafe { &*inst.gui_ext };
            if inst.gui_created {
                let shown = ext.show.is_some_and(|show| unsafe { show(inst.plugin) });
                if !shown {
                    let message = "The plugin could not show its editor".to_string();
                    inst.gui_error = Some(message.clone());
                    return Err(message);
                }
                self.gui_signals.open.store(true, Ordering::Relaxed);
                inst.gui_error = None;
                return Ok(());
            }

            let created = ext
                .create
                .is_some_and(|create| unsafe { create(inst.plugin, api.as_ptr(), true) });
            if !created {
                let message = format!(
                    "The plugin refused to create its floating editor ({})",
                    api.to_string_lossy()
                );
                inst.gui_error = Some(message.clone());
                return Err(message);
            }
            inst.gui_created = true;
            inst.gui_api = Some(api.to_string_lossy().into_owned());

            let mut width = 800u32;
            let mut height = 600u32;
            if let Some(get_size) = ext.get_size {
                if !unsafe { get_size(inst.plugin, &mut width, &mut height) }
                    || width == 0
                    || height == 0
                {
                    width = 800;
                    height = 600;
                }
            }
            self.gui_signals.width.store(width, Ordering::Relaxed);
            self.gui_signals.height.store(height, Ordering::Relaxed);

            let shown = ext.show.is_some_and(|show| unsafe { show(inst.plugin) });
            if !shown {
                if let Some(destroy) = ext.destroy {
                    unsafe { destroy(inst.plugin) };
                }
                inst.gui_created = false;
                inst.gui_api = None;
                let message = "The plugin created its editor but could not show it".to_string();
                inst.gui_error = Some(message.clone());
                return Err(message);
            }
            self.gui_signals.open.store(true, Ordering::Relaxed);
            inst.gui_error = None;
            log::info!("CLAP editor opened ({})", api.to_string_lossy());
            Ok(())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Err("This build was compiled without CLAP host support".to_string())
        }
    }

    /// Hide and destroy the plugin's editor. The plugin itself keeps running.
    pub fn close_gui(&mut self) {
        #[cfg(feature = "clap-host")]
        {
            if let Some(inst) = self.instance.as_mut() {
                if inst.gui_created && !inst.gui_ext.is_null() {
                    let ext = unsafe { &*inst.gui_ext };
                    if let Some(hide) = ext.hide {
                        unsafe { hide(inst.plugin) };
                    }
                    if let Some(destroy) = ext.destroy {
                        unsafe { destroy(inst.plugin) };
                    }
                }
                inst.gui_created = false;
                inst.gui_api = None;
            }
            self.gui_signals.open.store(false, Ordering::Relaxed);
            self.gui_signals.destroyed.store(false, Ordering::Relaxed);
        }
    }

    /// Resize the floating editor. Plugins that cannot resize return an error
    /// instead of silently ignoring the request.
    pub fn resize_gui(&mut self, width: u32, height: u32) -> Result<(), String> {
        #[cfg(feature = "clap-host")]
        {
            let Some(inst) = self.instance.as_mut() else {
                return Err("No CLAP plugin is loaded".to_string());
            };
            if !inst.gui_created || inst.gui_ext.is_null() {
                return Err("The plugin editor is not open".to_string());
            }
            let ext = unsafe { &*inst.gui_ext };
            if ext
                .can_resize
                .is_some_and(|can_resize| !unsafe { can_resize(inst.plugin) })
            {
                return Err("The plugin does not allow its editor to be resized".to_string());
            }
            let mut width = width.clamp(64, 8192);
            let mut height = height.clamp(64, 8192);
            if let Some(adjust) = ext.adjust_size {
                unsafe { adjust(inst.plugin, &mut width, &mut height) };
            }
            let Some(set_size) = ext.set_size else {
                return Err("The plugin does not implement editor resizing".to_string());
            };
            if !unsafe { set_size(inst.plugin, width, height) } {
                return Err("The plugin refused the editor size".to_string());
            }
            self.gui_signals.width.store(width, Ordering::Relaxed);
            self.gui_signals.height.store(height, Ordering::Relaxed);
            Ok(())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = (width, height);
            Err("This build was compiled without CLAP host support".to_string())
        }
    }

    /// Whether the plugin implements the `state` extension.
    pub fn has_state(&self) -> bool {
        #[cfg(feature = "clap-host")]
        {
            self.instance
                .as_ref()
                .is_some_and(|inst| !inst.state_ext.is_null())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            false
        }
    }

    /// Everything a host needs to draw the plugin rack.
    ///
    /// A plugin that failed mid-stream is reported here rather than on the
    /// audio thread: the block it failed was already discarded, and this is
    /// where the control path looks for what happened.
    pub fn rack_report(&self) -> ClapRackReport {
        let process_errors = self.process_errors.load(Ordering::Relaxed);
        ClapRackReport {
            plugin: self.describe(),
            params: self.param_infos(),
            path: self.source_path.clone(),
            bypassed: !self.enabled || process_errors > 0,
            has_state: self.has_state(),
            latency_samples: self.latency_samples,
            error: if process_errors > 0 {
                Some(format!(
                    "the plugin returned CLAP_PROCESS_ERROR {process_errors} time{}; it is out \
                     of the chain — reload it (or re-enable) to retry",
                    if process_errors == 1 { "" } else { "s" }
                ))
            } else {
                None
            },
            gui: self.gui_info(),
        }
    }

    /// Enumerate the plugins inside a CLAP library without keeping it loaded.
    ///
    /// A host must load the library to ask what it contains, but it need not
    /// instantiate anything: the factory describes every plugin up front. The
    /// library is unloaded again before returning, so probing a folder costs
    /// nothing but the probe.
    #[cfg(feature = "clap-host")]
    pub fn discover(path: &Path) -> Result<Vec<ClapPluginInfo>, String> {
        let lib = unsafe {
            libloading::Library::new(path)
                .map_err(|e| format!("Failed to dlopen CLAP library: {e}"))?
        };

        let entry_ptr: *const clap_plugin_entry = unsafe {
            let sym: libloading::Symbol<*const clap_plugin_entry> = lib
                .get(b"clap_entry\0")
                .map_err(|e| format!("Missing clap_entry symbol: {e}"))?;
            *sym
        };
        if entry_ptr.is_null() {
            return Err("clap_entry symbol pointer is null".to_string());
        }
        let entry = unsafe { &*entry_ptr };

        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {e}"))?;
        let init_ok = unsafe {
            match entry.init {
                Some(init) => init(c_path.as_ptr()),
                None => false,
            }
        };
        if !init_ok {
            return Err("CLAP entry init() returned false".to_string());
        }

        // Every exit from here on must run `deinit`, because `init` succeeded.
        let discovered = (|| -> Result<Vec<ClapPluginInfo>, String> {
            let factory_ptr = unsafe {
                match entry.get_factory {
                    Some(get_factory) => get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr()),
                    None => std::ptr::null(),
                }
            };
            if factory_ptr.is_null() {
                return Err("CLAP factory is null".to_string());
            }
            let factory = unsafe { &*(factory_ptr as *const clap_plugin_factory) };
            let get_count = factory
                .get_plugin_count
                .ok_or_else(|| "Factory has no get_plugin_count".to_string())?;
            let get_descriptor = factory
                .get_plugin_descriptor
                .ok_or_else(|| "Factory has no get_plugin_descriptor".to_string())?;

            let count = unsafe { get_count(factory) };
            let mut found = Vec::with_capacity(count as usize);
            for index in 0..count {
                let desc_ptr = unsafe { get_descriptor(factory, index) };
                if desc_ptr.is_null() {
                    continue;
                }
                found.push(plugin_info_from(unsafe { &*desc_ptr }));
            }
            Ok(found)
        })();

        if let Some(deinit) = entry.deinit {
            unsafe { deinit() };
        }
        discovered
    }

    /// Without the host feature there is no loader, but callers still get a
    /// reason rather than a silent empty list.
    #[cfg(not(feature = "clap-host"))]
    pub fn discover(_path: &std::path::Path) -> Result<Vec<ClapPluginInfo>, String> {
        Err("This build was compiled without CLAP host support".to_string())
    }

    /// Set a parameter value by ID.
    pub fn set_param(&mut self, id: u32, value: f64) {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref mut inst) = self.instance {
                inst.pending_param_events.push(param_event(id, value));
            }
            // A parameter only reaches a plugin as an event, and a plugin that
            // is not processing never sees one. Handing it over now keeps the
            // plugin's own `get_value` — and so anything a state capture records
            // — in step with the host's cache even while no audio flows.
            self.flush_pending_params();
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = (id, value);
        }
    }

    /// Apply a batch of parameter changes.
    pub fn apply_params(&mut self, batch: &[(u32, f64)]) {
        #[cfg(feature = "clap-host")]
        {
            for &(id, value) in batch {
                if let Some(ref mut inst) = self.instance {
                    inst.pending_param_events.push(param_event(id, value));
                }
            }
            // Once for the batch, not once per parameter.
            self.flush_pending_params();
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = batch;
        }
    }

    /// Hand the plugin every parameter the host has queued, without processing
    /// audio.
    ///
    /// CLAP offers `params.flush` for exactly this: the host calls it while it
    /// is not processing, and the plugin applies the events there and then. The
    /// queued events are deliberately *kept* when the plugin implements no
    /// `flush`, because the next `process` call would otherwise lose them.
    #[cfg(feature = "clap-host")]
    fn flush_pending_params(&mut self) {
        #[cfg(feature = "clap-host")]
        {
            let Some(inst) = self.instance.as_mut() else {
                return;
            };
            if inst.pending_param_events.is_empty()
                || inst.params_ext.is_null()
                || inst.plugin.is_null()
            {
                return;
            }
            let ext = unsafe { &*inst.params_ext };
            let Some(flush) = ext.flush else {
                return;
            };

            let in_events = clap_input_events {
                ctx: &inst.pending_param_events as *const _ as *mut c_void,
                size: Some(in_events_size),
                get: Some(in_events_get),
            };
            // A valid but empty output list: the host takes nothing back from a
            // flush, but a plugin is entitled to write into it and a null
            // pointer would make that undefined.
            let out_events = clap_output_events {
                ctx: std::ptr::null_mut(),
                try_push: None,
            };

            unsafe { flush(inst.plugin, &in_events, &out_events) };
            inst.pending_param_events.clear();
        }
    }

    /// Get current parameter value by ID.
    pub fn get_param_value(&self, _id: u32) -> Option<f64> {
        #[cfg(feature = "clap-host")]
        if let Some(ref inst) = self.instance {
            if !inst.params_ext.is_null() && !inst.plugin.is_null() {
                unsafe {
                    let ext = &*inst.params_ext;
                    if let Some(get_val) = ext.get_value {
                        let mut val = 0.0f64;
                        if get_val(inst.plugin, _id, &mut val) {
                            return Some(val);
                        }
                    }
                }
            }
        }
        None
    }

    /// Return total parameter count reported by the plugin.
    pub fn param_count(&self) -> u32 {
        #[cfg(feature = "clap-host")]
        if let Some(ref inst) = self.instance {
            if !inst.params_ext.is_null() && !inst.plugin.is_null() {
                unsafe {
                    let ext = &*inst.params_ext;
                    if let Some(count) = ext.count {
                        return count(inst.plugin);
                    }
                }
            }
        }
        0
    }
}
