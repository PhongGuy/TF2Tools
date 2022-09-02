class weaponInfo {
  name: string;
  alias: string;
  sounds: string[];
}

export const weaponSounds: weaponInfo[] = [
  {
    name: 'Rocket Lanucher',
    alias: 'Weapon_RPG',
    sounds: [
      'Single',
      'SingleCrit',
      'Draw',
      'Reload',
      'WorldReload'
    ]
  },
  {
    name: 'Direct Hit',
    alias: 'Weapon_RPG_DirectHit',
    sounds: [
      'Single',
      'SingleCrit',
      'Explode'
    ]
  }
];
