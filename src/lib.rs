use jni::JNIEnv;
use zygisk_rs::{Api, Module};

struct Zygisk {
    api: Api,
    env: JNIEnv<'static>,
}

impl Module for Zygisk {
    fn new(api: Api, env: *mut jni_sys::JNIEnv) -> Self {
        #[cfg(any(target_os = "android"))]
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info) // 使用 Trace 级别
                .with_tag("zygisk-frame-analyzer"),
        );

        let env = unsafe { JNIEnv::from_raw(env.cast()).unwrap() };

        Self { api, env }
    }

    fn pre_app_specialize(&mut self, args: &mut zygisk_rs::AppSpecializeArgs) {}

    fn post_app_specialize(&mut self, args: &zygisk_rs::AppSpecializeArgs) {}

    fn pre_server_specialize(&mut self, args: &mut zygisk_rs::ServerSpecializeArgs) {}

    fn post_server_specialize(&mut self, args: &zygisk_rs::ServerSpecializeArgs) {}
}
