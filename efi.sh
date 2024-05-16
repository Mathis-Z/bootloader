#!/bin/bash

EFI=/home/mathisz/Dokumente/hpi/BS2/bootloader/myboot/qemu/x86/esp/efi/boot/bootx64.efi
VDI_IMAGE_UUID=367e08ae-2feb-40b3-941e-7492ed12c8fb
EFI_VOL=vol1

EFI_FILE_TO_REPLACE=vbox_efi_vol/EFI/BOOT/BOOTX64.EFI # eigentlich wollen wir EFI/BOOT/BOOTX64.EFI ersetzen, aber das klappt irgendwie nicht :( vielleicht mal mit nem USB stick und echter hardware ausprobieren?
ORIGINAL_EFI_SAVE_LOCATION="$EFI_FILE_TO_REPLACE.ORIGINAL"


mount_efi() {
    sudo umount vbox_efi_vol 2>/dev/null
    sudo umount vbox_sysdisk 2>/dev/null

    mkdir vbox_sysdisk
    vboximg-mount -i $VDI_IMAGE_UUID -o allow_root vbox_sysdisk --rw

    if [ $? != 0 ]; then
        echo "mounting failed, exiting..."
        exit
    fi

    mkdir vbox_efi_vol
    sudo mount vbox_sysdisk/$EFI_VOL vbox_efi_vol
}

umount_efi() {
    sudo umount vbox_efi_vol 2>/dev/null
    sudo umount vbox_sysdisk 2>/dev/null

    # delete directories if empty
    if [ -z "$(ls -A vbox_sysdisk)" ]; then
        rm -r vbox_sysdisk
    fi
    if [ -z "$(ls -A vbox_efi_vol)" ]; then
        rm -r vbox_efi_vol
    fi
}

write_new_efi() {
    sudo cp -n $EFI_FILE_TO_REPLACE $ORIGINAL_EFI_SAVE_LOCATION
    sudo cp $EFI $EFI_FILE_TO_REPLACE
}

restore_old_efi() {
    sudo cp $ORIGINAL_EFI_SAVE_LOCATION $EFI_FILE_TO_REPLACE && sudo rm $ORIGINAL_EFI_SAVE_LOCATION
}

check_if_installed() {
    # Check the return code
    if ls "$ORIGINAL_EFI_SAVE_LOCATION"; then
        echo "custom EFI is installed"
    else
        echo "custom EFI is not installed"
    fi
}





# Check if the number of arguments provided is not equal to 1
if [ $# -ne 1 ]; then
    echo "Usage: $0 [install|uninstall|installed|mount|umount]"
    exit 1
fi

if [ $1 = "mount" ]; then
    mount_efi
    echo mounted
fi

if [ $1 = "umount" ]; then
    umount_efi
    echo unmounted
fi


if [ $1 = "install" ]; then
    mount_efi
    write_new_efi
    umount_efi
    echo installed
fi

if [ $1 = "uninstall" ]; then
    mount_efi
    restore_old_efi
    umount_efi
    echo uninstalled
fi

if [ $1 = "installed" ]; then
    mount_efi
    check_if_installed
    umount_efi
fi