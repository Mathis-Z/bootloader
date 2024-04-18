https://github.com/tianocore/tianocore.github.io/wiki/Getting-Started-Writing-Simple-Application

Setup with Linux and GCC:

https://github.com/tianocore/tianocore.github.io/wiki/Using-EDK-II-with-Native-GCC

https://github.com/tianocore/tianocore.github.io/wiki/Common-instructions

Mit efi.sh kann man nach einem build fix die .EFI updaten (wenn man die Variablen korrekt gesetzt hat)

Nicht vergessen in virtualbox in den settings unter System "EFI aktivieren" einzuschalten, sonst wird UEFI übersprungen oder so

efi.sh funktioniert manchmal nicht, wenn virtualbox läuft

http://kmmoore.github.io/articles/writing-a-uefi-application/

wenn du alles gebaut hast und so, dann musste in der HelloWorld.c noch nen while(1); einfügen, damit nach dem printen der Nachricht nicht sofort wieder der Boot Manager anspringt.
