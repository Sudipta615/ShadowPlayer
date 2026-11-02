use super::*;

#[test]
fn test_clap_host_node_lifecycle_without_plugin() {
    let mut node = ClapPluginHostNode::new(48000.0);
    assert!(!node.is_active());
    assert_eq!(node.latency_samples(), 0);

    let mut l = vec![1.0f32; 128];
    let mut r = vec![2.0f32; 128];
    let mut planes = [&mut l[..], &mut r[..]];

    node.process_block_f32(&mut planes);

    // Verification of pass-through behavior
    assert_eq!(l[0], 1.0);
    assert_eq!(r[0], 2.0);
}

#[cfg(feature = "clap-host")]
mod host_extensions {
    use super::*;

    #[test]
    fn the_host_advertises_only_the_extensions_it_implements() {
        let id = |text: &str| CString::new(text).unwrap();
        let log = id("clap.log");
        let thread_check = id("clap.thread-check");
        let params = id("clap.params");
        unsafe {
            assert!(
                !clap_host_get_extension(std::ptr::null(), log.as_ptr()).is_null(),
                "clap.log is implemented and must be offered"
            );
            assert!(
                !clap_host_get_extension(std::ptr::null(), thread_check.as_ptr()).is_null(),
                "clap.thread-check is implemented and must be offered"
            );
            assert!(
                clap_host_get_extension(std::ptr::null(), params.as_ptr()).is_null(),
                "an unimplemented extension must not be faked with an empty table"
            );
            assert!(clap_host_get_extension(std::ptr::null(), std::ptr::null()).is_null());
        }
    }

    #[test]
    fn the_thread_check_answers_about_the_thread_it_is_asked_from() {
        let context = Box::new(HostContext {
            main_thread: std::thread::current().id(),
            gui: Arc::new(ClapGuiSignals::default()),
        });
        let host = clap_host {
            clap_version: CLAP_VERSION,
            host_data: Box::as_ref(&context) as *const HostContext as *mut c_void,
            name: std::ptr::null(),
            vendor: std::ptr::null(),
            url: std::ptr::null(),
            version: std::ptr::null(),
            get_extension: None,
            request_restart: None,
            request_process: None,
            request_callback: None,
        };

        unsafe {
            assert!(clap_host_is_main_thread(&host));
            assert!(!clap_host_is_audio_thread(&host));
            IN_CLAP_PROCESS.with(|flag| flag.set(true));
            assert!(clap_host_is_audio_thread(&host));
            IN_CLAP_PROCESS.with(|flag| flag.set(false));
            assert!(!clap_host_is_audio_thread(&host));

            // A callback that is handed nothing must not guess.
            assert!(!clap_host_is_main_thread(std::ptr::null()));
        }
    }
}

/// A synthetic plugin that declares its main audio ports, so the probe can
/// be exercised without a library on disk.
#[cfg(feature = "clap-host")]
mod port_declaration {
    use super::*;

    static CHANNELS: AtomicU32 = AtomicU32::new(2);
    static IN_PLACE: AtomicBool = AtomicBool::new(true);

    unsafe extern "C" fn count(_plugin: *const clap_plugin, _is_input: bool) -> u32 {
        1
    }

    unsafe extern "C" fn get(
        _plugin: *const clap_plugin,
        _index: u32,
        is_input: bool,
        info: *mut clap_audio_port_info,
    ) -> bool {
        if info.is_null() {
            return false;
        }
        let mut declared = std::mem::zeroed::<clap_audio_port_info>();
        declared.id = if is_input { 1 } else { 2 };
        declared.flags = CLAP_AUDIO_PORT_IS_MAIN;
        declared.channel_count = CHANNELS.load(Ordering::Relaxed);
        declared.in_place_pair = if is_input || !IN_PLACE.load(Ordering::Relaxed) {
            CLAP_INVALID_ID
        } else {
            1
        };
        *info = declared;
        true
    }

    static PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
        count: Some(count),
        get: Some(get),
    };

    unsafe extern "C" fn get_extension(
        _plugin: *const clap_plugin,
        id: *const c_char,
    ) -> *const c_void {
        if id.is_null() {
            return std::ptr::null();
        }
        if CStr::from_ptr(id) == CLAP_EXT_AUDIO_PORTS {
            return &PORTS as *const clap_plugin_audio_ports as *const c_void;
        }
        std::ptr::null()
    }

    fn plugin() -> clap_plugin {
        let mut plugin = unsafe { std::mem::zeroed::<clap_plugin>() };
        plugin.get_extension = Some(get_extension);
        plugin
    }

    fn declaration(input: u32, output: u32, in_place: bool) -> MainPortDeclaration {
        MainPortDeclaration {
            input_channels: input,
            output_channels: output,
            in_place,
        }
    }

    #[test]
    fn the_main_bus_is_read_from_the_declaration_not_assumed() {
        let plugin = plugin();

        CHANNELS.store(2, Ordering::Relaxed);
        IN_PLACE.store(true, Ordering::Relaxed);
        assert_eq!(
            unsafe { probe_main_ports(&plugin) },
            Some(declaration(2, 2, true))
        );

        CHANNELS.store(1, Ordering::Relaxed);
        assert_eq!(
            unsafe { probe_main_ports(&plugin) },
            Some(declaration(1, 1, true))
        );

        CHANNELS.store(4, Ordering::Relaxed);
        assert_eq!(
            unsafe { probe_main_ports(&plugin) },
            Some(declaration(4, 4, true))
        );

        CHANNELS.store(2, Ordering::Relaxed);
        IN_PLACE.store(false, Ordering::Relaxed);
        assert_eq!(
            unsafe { probe_main_ports(&plugin) },
            Some(declaration(2, 2, false))
        );
    }

    #[test]
    fn a_plugin_without_the_audio_ports_extension_declares_nothing() {
        let mut plugin = unsafe { std::mem::zeroed::<clap_plugin>() };
        plugin.get_extension = None;
        assert_eq!(unsafe { probe_main_ports(&plugin) }, None);
    }
}
