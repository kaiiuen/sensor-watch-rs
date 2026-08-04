/* Memory layout for the Microchip SAM L22J18A (Sensor-Watch)
 *
 * 0x00000000-0x00002000: Bootloader       (0x2000 / 8192 bytes)
 * 0x00002000-0x0003C000: Firmware         (0x3A000 / 237568 bytes)
 * 0x0003C000-0x00040000: EEPROM Emulation (0x2000 / 8192 bytes)
 * 0x20000000-0x20008000: RAM              (0x8000 / 32768 bytes)
 */
MEMORY
{
  FLASH : ORIGIN = 0x00002000, LENGTH = 0x3A000
  RAM   : ORIGIN = 0x20000000, LENGTH = 0x8000
}
