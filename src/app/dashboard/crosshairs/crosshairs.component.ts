import { Component, OnInit, ViewChild } from '@angular/core';
import { MatListOption, MatSelectionList } from '@angular/material/list';
import { MatSelect } from '@angular/material/select';
import { MatTabGroup } from '@angular/material/tabs';
import { APP_CONFIG } from '../../../environments/environment';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { tfWeapons } from '../../mock/tfWeapons';
import { CrosshairSelected } from '../../models/crosshairSelected';
import { WeaponData } from '../../models/weaponData';
import { SnackService } from '../../services/snack.service';

/**
 * Crosshairs component
 */
@Component({
  selector: 'app-crosshairs',
  templateUrl: './crosshairs.component.html',
  styleUrls: ['./crosshairs.component.scss']
})
export class CrosshairsComponent implements OnInit {

  /**
   * Crosshair selected `MatSelect`
   */
  @ViewChild('crosshairSelected') crosshairSelected: MatSelect;
  /**
   * Crosshairs tab `MatTabGroup`
   */
  @ViewChild('crosshairsTab') crosshairsTab: MatTabGroup;
  /**
   * Crosshair background `MatSelect`
   */
  @ViewChild('background') background: MatSelect;

  // classes
  /**
   * Scout list `MatSelectionList`
   */
  @ViewChild('scout') scout: MatSelectionList;
  /**
   * Soldier list `MatSelectionList`
   */
  @ViewChild('soldier') soldier: MatSelectionList;
  /**
   * Pyro list `MatSelectionList`
   */
  @ViewChild('pyro') pyro: MatSelectionList;
  /**
   * Demoman list `MatSelectionList`
   */
  @ViewChild('demoman') demoman: MatSelectionList;
  /**
   * Heavy list `MatSelectionList`
   */
  @ViewChild('heavy') heavy: MatSelectionList;
  /**
   * Engineer list `MatSelectionList`
   */
  @ViewChild('engineer') engineer: MatSelectionList;
  /**
   * Medic list `MatSelectionList`
   */
  @ViewChild('medic') medic: MatSelectionList;
  /**
   * Sniper list `MatSelectionList`
   */
  @ViewChild('sniper') sniper: MatSelectionList;
  /**
   * Spy list `MatSelectionList`
   */
  @ViewChild('spy') spy: MatSelectionList;

  /**
   * Selected of crosshairs component
   */
  selected = new CrosshairSelected();
  /**
   * Selected weapons of crosshairs component
   */
  selectedWeapons: WeaponData[] = [];

  /**
   * All weapons of crosshairs component
   */
  allWeapons: WeaponData[] = [];
  // classes
  /**
   * Scout weapons of crosshairs component
   */
  scoutWeapons: WeaponData[] = [];
  /**
   * Soldier weapons of crosshairs component
   */
  soldierWeapons: WeaponData[] = [];
  /**
   * Pyro weapons of crosshairs component
   */
  pyroWeapons: WeaponData[] = [];
  /**
   * Demoman weapons of crosshairs component
   */
  demomanWeapons: WeaponData[] = [];
  /**
   * Heavy weapons of crosshairs component
   */
  heavyWeapons: WeaponData[] = [];
  /**
   * Engineer weapons of crosshairs component
   */
  engineerWeapons: WeaponData[] = [];
  /**
   * Medic weapons of crosshairs component
   */
  medicWeapons: WeaponData[] = [];
  /**
   * Sniper weapons of crosshairs component
   */
  sniperWeapons: WeaponData[] = [];
  /**
   * Spy weapons of crosshairs component
   */
  spyWeapons: WeaponData[] = [];

  /**
   * Apply to
   */
  applyTo = 'Scout';

  /**
   * Creates an instance of crosshairs component.
   *
   * @param app AppComponent
   * @param electron
   * @param snack
   */
  constructor(
    public app: AppComponent,
    private electron: ElectronService,
    private snack: SnackService,
  ) { }

  /**
   * on init
   */
  ngOnInit(): void {
    this.app.update('vtf');
    this.get();
  }

  /**
   * Selected items
   *
   * @param items
   */
  selectedItems(items: MatListOption[]): void {
    this.selected = new CrosshairSelected();
    this.selectedWeapons = [];
    items.forEach(d => {
      const weapon = d.value as WeaponData;
      this.selectedWeapons.push(weapon);
      if (!this.selected.classes.includes(weapon.info.class)) {
        this.selected.classes.push(weapon.info.class);
      }
      this.selected.names.push(weapon.info.name);
      weapon.info.weaponsAffected.forEach(effected => {
        this.selected.weaponsAffected.push(effected);
      });

      // this.selected.weaponClasses.push(weapon.info.weaponClass);
      // if (!this.selected.slots.includes(weapon.info.slot)) {
      //   this.selected.slots.push(weapon.info.slot);
      // }

      this.crosshairSelected.value = weapon.crosshair;
    });
  }

  /**
   * Un select all
   */
  unSelectAll() {
    this.scout.deselectAll();
    this.soldier.deselectAll();
    this.pyro.deselectAll();
    this.demoman.deselectAll();
    this.heavy.deselectAll();
    this.engineer.deselectAll();
    this.medic.deselectAll();
    this.sniper.deselectAll();
    this.spy.deselectAll();
    this.selectedItems([]);

    if (this.crosshairsTab.selectedIndex === 0) {
      this.applyTo = 'Scout';
    } else if (this.crosshairsTab.selectedIndex === 1) {
      this.applyTo = 'Soldier';
    } else if (this.crosshairsTab.selectedIndex === 2) {
      this.applyTo = 'Pyro';
    } else if (this.crosshairsTab.selectedIndex === 3) {
      this.applyTo = 'Demoman';
    } else if (this.crosshairsTab.selectedIndex === 4) {
      this.applyTo = 'Heavy';
    } else if (this.crosshairsTab.selectedIndex === 5) {
      this.applyTo = 'Engineer';
    } else if (this.crosshairsTab.selectedIndex === 6) {
      this.applyTo = 'Medic';
    } else if (this.crosshairsTab.selectedIndex === 7) {
      this.applyTo = 'Sniper';
    } else if (this.crosshairsTab.selectedIndex === 8) {
      this.applyTo = 'Spy';
    }
  }

  /**
   * Generates vtf crosshairs
   */
  generate(): void {
    const vtfPath = `${this.app.settings.customPath}\\mycustomstuff\\materials\\vgui\\replay\\thumbnails`;
    const scriptPath = `${this.app.settings.customPath}\\mycustomstuff\\scripts`;
    this.electron.fs.ensureDir(vtfPath)
      .then(() => {
        this.snack.show('Adding vtf crosshairs', null, 2500);
        this.electron.fs.copy(`${APP_CONFIG.src}assets/vtf/materials/vgui/replay/thumbnails`, vtfPath, { overwrite: true })
          .then(() => {
            this.electron.fs.ensureDir(scriptPath)
              .then(() => {
                this.electron.fs.copy(`${APP_CONFIG.src}assets/vtf/scripts`, scriptPath, { overwrite: false })
                  .then(() => {
                    this.snack.show('vtf crosshairs was generated');
                    this.app.update();
                  })
                  .catch((err) => this.app.error(err));
              })
              .catch((err) => this.app.error(err));
          })
          .catch((err) => this.app.error(err));
      })
      .catch((err) => this.app.error(err));
  }

  /**
   * Applies to weapons
   */
  apply(): void {
    const crosshair = this.crosshairSelected.value;
    this.selectedWeapons.forEach(weapon => {
      const file = this.electron.fs.readFileSync(weapon.path, { encoding: 'utf8', flag: 'r' });
      this.app.log.next(`Installing crosshair: *CHANGE* "${weapon.crosshair}" => "${crosshair}" in "${weapon.path}"`);
      this.electron.fs.writeFile(weapon.path, file.replace(`/thumbnails/${weapon.crosshair}`, `/thumbnails/${crosshair}`))
        .then(() => {
          this.snack.show(`Updated ${weapon.info.name} to ${crosshair}`);
          this.app.update('vtf');
          this.get();
        })
        .catch((err) => this.app.error(err));
    });
  }

  /**
   * Applies to all weapons
   *
   * @param slot
   */
  applyAll(slot: 'Primary' | 'Secondary' | 'Melee' | 'All'): void {
    const crosshair = this.crosshairSelected.value;
    let weaponsTab: WeaponData[] = [];

    if (this.crosshairsTab.selectedIndex === 0) {
      weaponsTab = this.scoutWeapons;
    } else if (this.crosshairsTab.selectedIndex === 1) {
      weaponsTab = this.soldierWeapons;
    } else if (this.crosshairsTab.selectedIndex === 2) {
      weaponsTab = this.pyroWeapons;
    } else if (this.crosshairsTab.selectedIndex === 3) {
      weaponsTab = this.demomanWeapons;
    } else if (this.crosshairsTab.selectedIndex === 4) {
      weaponsTab = this.heavyWeapons;
    } else if (this.crosshairsTab.selectedIndex === 5) {
      weaponsTab = this.engineerWeapons;
    } else if (this.crosshairsTab.selectedIndex === 6) {
      weaponsTab = this.medicWeapons;
    } else if (this.crosshairsTab.selectedIndex === 7) {
      weaponsTab = this.sniperWeapons;
    } else if (this.crosshairsTab.selectedIndex === 8) {
      weaponsTab = this.spyWeapons;
    }

    weaponsTab.forEach(weapon => {
      if (weapon !== crosshair) {
        if (weapon.info.slot === slot || slot === 'All') {
          const file = this.electron.fs.readFileSync(weapon.path, { encoding: 'utf8', flag: 'r' });
          this.app.log.next(`Installing crosshair: *CHANGE* "${weapon.crosshair}" => "${crosshair}" in "${weapon.path}"`);
          this.electron.fs.writeFileSync(weapon.path, file.replace(`/thumbnails/${weapon.crosshair}`, `/thumbnails/${crosshair}`));
          this.snack.show(`Updated ${weapon.info.name} to ${crosshair}`, null, 800);
        }
      }
    });
    this.app.update('vtf');
    this.get();
  }

  /**
   * Applies to all weapons on all classes
   *
   * @param slot
   */
  applyAllClasses(slot: 'Primary' | 'Secondary' | 'Melee' | 'All'): void {
    const crosshair = this.crosshairSelected.value;
    this.allWeapons.forEach(weapon => {
      if (weapon !== crosshair) {
        if (weapon.info.slot === slot || weapon.info.slot === 'All') {
          const file = this.electron.fs.readFileSync(weapon.path, { encoding: 'utf8', flag: 'r' });
          this.app.log.next(`Installing crosshair: *CHANGE* "${weapon.crosshair}" => "${crosshair}" in "${weapon.path}"`);
          this.electron.fs.writeFileSync(weapon.path, file.replace(`/thumbnails/${weapon.crosshair}`, `/thumbnails/${crosshair}`));
        }
      }
    });
    this.snack.show(`Updated ${slot} weapons to ${crosshair} on all classes`, null, 4000);
    this.app.update('vtf');
    this.get();
  }

  /**
   * Changes background
   *
   * @param v
   */
  changeBackground(v: string) {
    this.app.settings.crosshairBackground = v;
    this.app.log.next(`Crosshair background: *CHANGE* "${v}"`);
    this.app.settingsUpdate.next(this.app.settings);
  }

  /**
   * Gets crosshairs component
   */
  private get(): void {

    this.scoutWeapons = [];
    this.soldierWeapons = [];
    this.pyroWeapons = [];
    this.demomanWeapons = [];
    this.heavyWeapons = [];
    this.engineerWeapons = [];
    this.medicWeapons = [];
    this.sniperWeapons = [];
    this.spyWeapons = [];

    this.app.vtfScripts.forEach(path => {
      // read script file
      const read = this.electron.fs.readFileSync(path, { encoding: 'utf8', flag: 'r' });
      const name = path.replace('.txt', '').split('\\').pop();

      const crosshair = read
        .replace(/\t/g, '')
        .replace(/\r/g, '')
        .split('\n')
        .filter(a => a.includes('vgui/replay/thumbnails'))
        .join('')
        .replace(/"file""vgui\/replay\/thumbnails\//g, '')
        .replace(/"/g, '');

      const newWeapon = new WeaponData();
      newWeapon.crosshair = crosshair;
      newWeapon.path = path;

      tfWeapons.forEach((v, k) => {
        if (name === v.weaponClass) {
          newWeapon.info = v;

          if (v.class === 'Scout') {
            this.scoutWeapons.push(newWeapon);
          } else if (v.class === 'Soldier') {
            this.soldierWeapons.push(newWeapon);
          } else if (v.class === 'Pyro') {
            this.pyroWeapons.push(newWeapon);
          } else if (v.class === 'Demoman') {
            this.demomanWeapons.push(newWeapon);
          } else if (v.class === 'Heavy') {
            this.heavyWeapons.push(newWeapon);
          } else if (v.class === 'Engineer') {
            this.engineerWeapons.push(newWeapon);
          } else if (v.class === 'Medic') {
            this.medicWeapons.push(newWeapon);
          } else if (v.class === 'Sniper') {
            this.sniperWeapons.push(newWeapon);
          } else if (v.class === 'Spy') {
            this.spyWeapons.push(newWeapon);
          }

          // add all to one
          this.allWeapons.push(newWeapon);
        }
      });
    });

    this.scoutWeapons.sort(this.weaponSort);
    this.soldierWeapons.sort(this.weaponSort);
    this.pyroWeapons.sort(this.weaponSort);
    this.demomanWeapons.sort(this.weaponSort);
    this.heavyWeapons.sort(this.weaponSort);
    this.engineerWeapons.sort(this.weaponSort);
    this.medicWeapons.sort(this.weaponSort);
    this.sniperWeapons.sort(this.weaponSort);
    this.spyWeapons.sort(this.weaponSort);

    // unselect all
    this.selectedItems([]);
  }

  /**
   * Weapons sort
   *
   * @param a
   * @param b
   * @returns
   */
  private weaponSort(a: WeaponData, b: WeaponData) {
    if (a.info.slot === 'Primary' && b.info.slot !== 'Primary') {
      return -1;
    }
    if (a.info.slot === 'Secondary' && b.info.slot !== 'Primary' && b.info.slot !== 'Secondary') {
      return -1;
    }
    return 0;

  }
}
