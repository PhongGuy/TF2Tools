import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import { BehaviorSubject } from 'rxjs';
import { ElectronService } from './core/services';
import { Settings } from './models/settings';
import { FileHelpService } from './services/file-help.service';
import { SnackService } from './services/snack.service';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit {

  @ViewChild('room') room: ElementRef;

  public settings = new Settings();
  settingsUpdate: BehaviorSubject<Settings> = new BehaviorSubject<Settings>(this.settings);

  loading = true;
  public appTemp: string;
  fullscreenIcon = 'fullscreen';
  fullscreenTip = 'Maximize';

  // things used through out the app
  hitsounds: string[] = [];
  killsounds: string[] = [];
  vtf: string[] = [];
  vtfScripts: string[] = [];
  huds: string[] = [];
  weponSounds = '';

  private appdata: string;
  private fullscreen = false;
  private defaultCustomPath = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2\\tf\\custom';
  private defaultLibraryPath = this.electron.appData('TF2Tools\\Library');

  constructor(
    private electron: ElectronService,
    private router: Router,
    private snack: SnackService,
    private fileHelp: FileHelpService
  ) {
    this.appdata = this.electron.appData('TF2Tools');
    this.appTemp = `${this.appdata}\\temp`;

    this.electron.fs.ensureDir(this.appdata);
    this.electron.fs.ensureDir(`${this.appdata}\\temp`);


    if (this.electron.fs.existsSync(`${this.appdata}\\settings.json`)) {
      const data = this.electron.fs.readFileSync(`${this.appdata}\\settings.json`, { encoding: 'utf8', flag: 'r' });
      const jsonSettings = JSON.parse(data) as Settings;
      for (const k in this.settings) {
        if (Object.prototype.hasOwnProperty.call(this.settings, k) && Object.prototype.hasOwnProperty.call(jsonSettings, k)) {
          this.settings[k] = jsonSettings[k];
        }
      }
    }

    if (this.settings.libraryPath !== null) {
      if (!this.electron.fs.existsSync(this.settings.libraryPath)) {
        this.settings.libraryPath = this.defaultLibraryPath;
      }
    } else {
      this.settings.libraryPath = this.defaultLibraryPath;
    }

    if (this.settings.customPath !== null) {
      if (this.electron.fs.existsSync(this.settings.customPath)) {
        this.router.navigate(['dashboard/hud']);
      } else {
        if (this.electron.fs.existsSync(this.defaultCustomPath)) {
          this.settings.customPath = this.defaultCustomPath;
          this.router.navigate(['dashboard/hud']);
        } else {
          this.router.navigate(['setup']);
        }
      }
    } else {
      if (this.electron.fs.existsSync(this.defaultCustomPath)) {
        this.settings.customPath = this.defaultCustomPath;
        this.router.navigate(['dashboard/hud']);
      } else {
        this.router.navigate(['setup']);
      }
    }
  }

  /**
   * When we want to add mastercomfig, this is the url to the latest relese
   * https://github.com/mastercomfig/mastercomfig/releases/latest/download/mastercomfig-low-preset.vpk
   */

  /**
   * Wehn we want to add version update, this is the url to get the latest version
   * https://api.github.com/repos/PhongGuy/TF2Tools/releases/latest
   */

  ngOnInit() {

    // scroll to top when we change navigation
    this.router.events.subscribe(event => {
      if (event instanceof NavigationEnd) {
        this.room.nativeElement.scrollTop = 0;
      }
    });

    this.loading = false;

    // when we update settings we write them to json file
    this.settingsUpdate.subscribe(a => {
      this.electron.fs.writeFileSync(`${this.appdata}\\settings.json`, JSON.stringify(this.settings));
    });
  }

  async update(what: 'huds' | 'hitsounds' | 'vtf' | 'weaponSounds' | null = null) {

    // reset
    if (what === 'huds') {
      this.huds = [];
    } else if (what === 'hitsounds') {
      this.hitsounds = [];
      this.killsounds = [];
    } else if (what === 'vtf') {
      this.vtf = [];
      this.vtfScripts = [];
    } else if (what === 'weaponSounds') {
      this.weponSounds = null;
    } else {
      this.huds = [];
      this.hitsounds = [];
      this.killsounds = [];
      this.vtf = [];
      this.weponSounds = null;
    }

    // get all files in the custom folder
    const customDir = this.fileHelp.getAllFiles(this.settings.customPath);

    // Try to find all huds
    if (what === 'huds' || what === 'vtf' || what === null) {
      customDir.forEach(file => {
        if (file.endsWith('info.vdf')) {
          const hudPath = file.split('\\');
          hudPath.pop();
          this.huds.push(hudPath.join('\\'));
        }
      });
    }

    customDir.forEach(file => {
      // try to find hitsounds
      if (what === 'hitsounds' || what === null) {
        if (file.endsWith('hitsound.wav')) {
          this.hitsounds.push(file);
        }

        // try to find killsounds
        if (file.endsWith('killsound.wav')) {
          this.killsounds.push(file);
        }
      }

      // try to find vtf crosshairs
      if (what === 'vtf' || what === null) {
        if (file.includes('materials\\vgui\\replay\\thumbnails') && file.endsWith('.vtf')) {
          let err = 0;
          for (const hud of this.huds) {
            if (file.startsWith(hud)) {
              err++;
            }
          }
          if (err === 0) {
            this.vtf.push(file);
          }
        }

        if (file.includes('scripts\\tf_weapon') && file.endsWith('.txt')) {
          this.vtfScripts.push(file);
        }
      }

      // try to find weapon sounds
      if (what === 'weaponSounds' || what === null) {
        if (file.endsWith('game_sounds_weapons.txt')) {
          this.weponSounds = file;
        }
      }
    });
  }

  /** tell electron to minimize  */
  minimize() {
    this.electron.ipcRenderer.send('minimize');
  }

  /** toggle between full screen and minimize */
  fullscreenToggle() {
    this.electron.ipcRenderer.send('fullscreen');
    this.fullscreen = this.fullscreen ? false : true;
    this.fullscreenTip = this.fullscreen ? 'Restore Down' : 'Maximize';
    this.fullscreenIcon = this.fullscreen ? 'fullscreen_exit' : 'fullscreen';
  }

  /** write the settings to json and close the application */
  close() {
    this.electron.fs.writeFileSync(`${this.appdata}\\settings.json`, JSON.stringify(this.settings));
    window.close();
  }

  error(err: any): void {
    console.error(err);
    this.snack.show(err);
  }

  generateRandomString(length) {
    let result = ''; let seeds;

    for (let i = 0; i < length - 1; i++) {
      //Generate seeds array, that will be the bag from where randomly select generated char
      seeds = [
        Math.floor(Math.random() * 10) + 48,
        Math.floor(Math.random() * 25) + 65,
        Math.floor(Math.random() * 25) + 97
      ];

      //Choise randomly from seeds, convert to char and append to result
      result += String.fromCharCode(seeds[Math.floor(Math.random() * 3)]);
    }

    return result;
  }


}
