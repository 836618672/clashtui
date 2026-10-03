# Manual ClashTui configuration (Mihomo)

Only Mihomo is supported. Use an isolated VM for real installation, service and TUN acceptance. The [test pipeline](../testing/test_pipeline_zh.md) uses temporary data and mocks for safe local checks.

Prepare platform-compatible ClashTui and Mihomo binaries. ClashTui can be built with cargo build --release --locked. Create separate application and core configuration directories; copy contrib/default_configs/config.yaml and set absolute paths in mihomo.core.

```yaml
mihomo:
  core:
    bin_path: /absolute/path/to/mihomo
    config_dir: /absolute/path/to/core-config
    config_path: /absolute/path/to/core-config/config.yaml
  core_service:
    service_name: clashtui_mihomo
    is_user: true
    service_controller: systemd
timeout: 5
extra:
  edit_cmd:
  open_dir_cmd:
```

Windows uses mihomo.exe and NSSM. Remove service_controller on Windows/macOS to use platform defaults. The registered service must match service_name.

Create mihomo/profiles and mihomo/templates beneath the application directory. Store core_override_config.yaml in its mihomo directory. Refer to contrib/default_configs/mihomo/core_override_config_no_tun.yaml for a configuration without TUN. Verify controller address, secret, proxy ports and DNS settings; the running configuration and override must agree on controller credentials.

```sh
clashtui --config-dir=/absolute/path/to/clashtui-test
clashtui --config-dir=/absolute/path/to/clashtui-test manage state
```

Register services in the VM using systemd/OpenRC on Linux, launchd on macOS or NSSM on Windows. The repository installer contains the corresponding Mihomo service generation. Ensure the service account can read the configuration and write core caches.

Use clashtui service status/start/stop/restart only on the configured test machine. Cross-core switching has been removed. Templates are available under contrib/templates/mihomo; copy them to the application template directory before preview or generation.

The Web management token is separate from the core secret. See [Web setup and acceptance](../testing/tui_web_testing_zh.md) for authentication, panel deployment and controller migration. Record real-core versions and recovery evidence separately from mock tests.
