use plugin_interfaces::{create_plugin_interface_from_handler, PluginHandler, PluginInterface};

mod api;
mod config;
mod history;
mod plugin;

pub use plugin::DeepSeekPlugin;

/// 创建插件实例的导出函数
#[no_mangle]
pub extern "C" fn create_plugin() -> *mut PluginInterface {
    let plugin = DeepSeekPlugin::new();
    let handler: Box<dyn PluginHandler> = Box::new(plugin);
    create_plugin_interface_from_handler(handler)
}

/// 销毁插件实例的导出函数
#[no_mangle]
pub extern "C" fn destroy_plugin(interface: *mut PluginInterface) {
    if !interface.is_null() {
        unsafe {
            ((*interface).destroy)((*interface).plugin_ptr);
            let _ = Box::from_raw(interface);
        }
    }
}
