use super::*;

impl ClapPluginHostNode {
    /// Creates a new inactive CLAP plugin host node.
    pub fn new(sample_rate: f32) -> Self {
        #[cfg(feature = "clap-host")]
        {
            // The extension callbacks read the node's thread identity through
            // `host_data`; the box's address is what must stay stable.
            let gui_signals = Arc::new(ClapGuiSignals::default());
            let host_context = Box::new(HostContext {
                main_thread: std::thread::current().id(),
                gui: Arc::clone(&gui_signals),
            });
            let host_descriptor = Box::new(clap_host {
                clap_version: CLAP_VERSION,
                host_data: Box::as_ref(&host_context) as *const HostContext as *mut c_void,
                name: c"audio-engine".as_ptr(),
                vendor: c"Sudipta615".as_ptr(),
                url: c"https://github.com/Sudipta615/audio_engine".as_ptr(),
                version: c"1.0.0".as_ptr(),
                get_extension: Some(clap_host_get_extension),
                request_restart: Some(clap_host_request_restart),
                request_process: Some(clap_host_request_process),
                request_callback: Some(clap_host_request_callback),
            });

            Self {
                sample_rate,
                enabled: true,
                latency_samples: 0,
                source_path: None,
                process_errors: AtomicU32::new(0),
                instance: None,
                host_descriptor,
                _host_context: Some(host_context),
                gui_signals,
                f64_scratch: [
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                ],
            }
        }

        #[cfg(not(feature = "clap-host"))]
        {
            Self {
                sample_rate,
                enabled: true,
                latency_samples: 0,
                source_path: None,
                process_errors: AtomicU32::new(0),
            }
        }
    }

    /// Loads and instantiates a CLAP plugin from a dynamic library file path (control thread).
    #[cfg(feature = "clap-host")]
    #[allow(dead_code)]
    pub fn load_plugin(&mut self, path: &Path, plugin_id: Option<&str>) -> Result<(), String> {
        let lib = unsafe {
            libloading::Library::new(path)
                .map_err(|e| format!("Failed to dlopen CLAP library: {e}"))?
        };

        // Standard CLAP entry point: "clap_entry" symbol
        let entry_ptr: *const clap_plugin_entry = unsafe {
            let sym: libloading::Symbol<*const clap_plugin_entry> = lib
                .get(b"clap_entry\0")
                .map_err(|e| format!("Missing clap_entry symbol: {e}"))?;
            *sym
        };

        if entry_ptr.is_null() {
            return Err("clap_entry symbol pointer is null".to_string());
        }

        let entry_ref = unsafe { &*entry_ptr };
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {e}"))?;

        let init_ok = unsafe {
            if let Some(init) = entry_ref.init {
                init(c_path.as_ptr())
            } else {
                false
            }
        };

        if !init_ok {
            return Err("CLAP entry init() returned false".to_string());
        }

        let factory_ptr = unsafe {
            if let Some(get_factory) = entry_ref.get_factory {
                get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr())
            } else {
                std::ptr::null()
            }
        };

        if factory_ptr.is_null() {
            return Err("CLAP factory is null".to_string());
        }

        let factory = unsafe { &*(factory_ptr as *const clap_plugin_factory) };
        let count = unsafe {
            if let Some(get_plugin_count) = factory.get_plugin_count {
                get_plugin_count(factory)
            } else {
                0
            }
        };

        if count == 0 {
            return Err("CLAP factory contains no plugins".to_string());
        }

        // Select plugin ID
        let target_id = if let Some(id) = plugin_id {
            CString::new(id).map_err(|e| format!("Invalid plugin ID: {e}"))?
        } else {
            // Pick first plugin
            let desc_ptr = unsafe {
                if let Some(get_plugin_descriptor) = factory.get_plugin_descriptor {
                    get_plugin_descriptor(factory, 0)
                } else {
                    std::ptr::null()
                }
            };
            if desc_ptr.is_null() {
                return Err("Failed to get first CLAP plugin descriptor".to_string());
            }
            let desc = unsafe { &*desc_ptr };
            unsafe { CStr::from_ptr(desc.id).to_owned() }
        };

        let host_ptr: *const clap_host = &*self.host_descriptor;
        let plugin_ptr = unsafe {
            if let Some(create_plugin) = factory.create_plugin {
                create_plugin(factory, host_ptr, target_id.as_ptr())
            } else {
                std::ptr::null()
            }
        };

        if plugin_ptr.is_null() {
            return Err("create_plugin returned null".to_string());
        }

        let plugin = unsafe { &*plugin_ptr };
        let initialized = unsafe {
            if let Some(init) = plugin.init {
                init(plugin_ptr)
            } else {
                false
            }
        };

        if !initialized {
            unsafe {
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin init() returned false".to_string());
        }

        // What the plugin's own port declaration says about the bus it needs.
        // This node feeds exactly two channels and processes in place when the
        // plugin permits it; a wider or asymmetric declaration cannot be
        // honored, and pretending otherwise would hand the plugin pointers it
        // was told not to expect.
        let (channels, in_place) = match unsafe { probe_main_ports(plugin_ptr) } {
            Some(declaration)
                if declaration.input_channels > 2 || declaration.output_channels > 2 =>
            {
                unsafe {
                    if let Some(destroy) = plugin.destroy {
                        destroy(plugin_ptr);
                    }
                }
                return Err(format!(
                    "the plugin's main ports are {} in / {} out; this host feeds a stereo bus",
                    declaration.input_channels, declaration.output_channels
                ));
            }
            Some(declaration) if declaration.input_channels != declaration.output_channels => {
                unsafe {
                    if let Some(destroy) = plugin.destroy {
                        destroy(plugin_ptr);
                    }
                }
                return Err(format!(
                    "the plugin maps {} input channel(s) to {} output channel(s); this host \
                     processes in place and needs matching ports",
                    declaration.input_channels, declaration.output_channels
                ));
            }
            Some(declaration) if declaration.input_channels == 1 => {
                log::warn!(
                    "CLAP plugin's main ports are mono; only channel 0 is processed and \
                     channel 1 passes through untouched"
                );
                (1usize, declaration.in_place)
            }
            Some(declaration) => (declaration.input_channels as usize, declaration.in_place),
            None => {
                // `clap.audio-ports` is optional; without a declaration the
                // only honest answer left is the stereo, in-place bus the
                // graph provides.
                (2usize, true)
            }
        };

        let activated = unsafe {
            if let Some(activate) = plugin.activate {
                activate(
                    plugin_ptr,
                    self.sample_rate as f64,
                    32,
                    MAX_AUDIO_BLOCK_FRAMES as u32,
                )
            } else {
                false
            }
        };

        if !activated {
            unsafe {
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin activate() returned false".to_string());
        }

        // A conforming plugin stays silent until it is told processing has
        // started, so activating without this would host a mute plugin.
        let processing = unsafe {
            match plugin.start_processing {
                Some(start) => start(plugin_ptr),
                None => true,
            }
        };
        if !processing {
            unsafe {
                if let Some(deactivate) = plugin.deactivate {
                    deactivate(plugin_ptr);
                }
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin start_processing() returned false".to_string());
        }

        let params_ext = unsafe {
            if let Some(get_ext) = plugin.get_extension {
                get_ext(plugin_ptr, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params
            } else {
                std::ptr::null()
            }
        };

        let state_ext = unsafe {
            if let Some(get_ext) = plugin.get_extension {
                get_ext(plugin_ptr, CLAP_EXT_STATE.as_ptr()) as *const clap_plugin_state
            } else {
                std::ptr::null()
            }
        };

        let gui_ext = unsafe {
            if let Some(get_ext) = plugin.get_extension {
                get_ext(plugin_ptr, CLAP_EXT_GUI.as_ptr()) as *const clap_plugin_gui
            } else {
                std::ptr::null()
            }
        };

        let lib_arc = Arc::new(ClapLibrary {
            _lib: lib,
            _entry: entry_ptr,
        });

        self.source_path = Some(path.to_string_lossy().to_string());

        // A fresh instance is a fresh start: a failure latch from the plugin
        // that was loaded before it does not carry over.
        self.process_errors.store(0, Ordering::Relaxed);

        self.instance = Some(ClapInstance {
            _library: lib_arc,
            plugin: plugin_ptr,
            active: true,
            processing: true,
            _sample_rate: self.sample_rate as f64,
            _max_frames: MAX_AUDIO_BLOCK_FRAMES,
            channels,
            in_place,
            out_scratch: if in_place {
                [Vec::new(), Vec::new()]
            } else {
                [
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                ]
            },
            params_ext,
            state_ext,
            gui_ext,
            gui_created: false,
            gui_api: None,
            gui_error: None,
            pending_param_events: Vec::with_capacity(32),
        });

        log::info!(
            "Successfully loaded and activated CLAP plugin: {:?}",
            target_id
        );
        Ok(())
    }
}

impl ClapPluginHostNode {
    /// Serialize current plugin state to an opaque byte buffer.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref inst) = self.instance {
                if inst.state_ext.is_null() || inst.plugin.is_null() {
                    return Err("Plugin does not implement state extension".to_string());
                }
                let mut writer = ClapStreamWriter { data: Vec::new() };
                let ostream = clap_ostream {
                    ctx: &mut writer as *mut _ as *mut c_void,
                    write: Some(stream_write_fn),
                };
                unsafe {
                    let ext = &*inst.state_ext;
                    if let Some(save) = ext.save {
                        if save(inst.plugin, &ostream) {
                            return Ok(writer.data);
                        } else {
                            return Err("CLAP state save() returned false".to_string());
                        }
                    }
                }
            }
            Err("No active plugin instance to save state from".to_string())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Err("clap-host feature disabled".to_string())
        }
    }

    /// Restore plugin state from an opaque byte buffer.
    pub fn load_state(&mut self, _state_bytes: &[u8]) -> Result<(), String> {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref mut inst) = self.instance {
                if inst.state_ext.is_null() || inst.plugin.is_null() {
                    return Err("Plugin does not implement state extension".to_string());
                }
                let mut reader = ClapStreamReader {
                    data: _state_bytes,
                    pos: 0,
                };
                let istream = clap_istream {
                    ctx: &mut reader as *mut _ as *mut c_void,
                    read: Some(stream_read_fn),
                };
                unsafe {
                    let ext = &*inst.state_ext;
                    if let Some(load) = ext.load {
                        if load(inst.plugin, &istream) {
                            return Ok(());
                        } else {
                            return Err("CLAP state load() returned false".to_string());
                        }
                    }
                }
            }
            Err("No active plugin instance to load state into".to_string())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Err("clap-host feature disabled".to_string())
        }
    }
}
