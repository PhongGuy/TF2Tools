import { Component, OnInit } from '@angular/core';
import { Router } from '@angular/router';
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

  public settings = new Settings();
  settingsUpdate: BehaviorSubject<Settings> = new BehaviorSubject<Settings>(this.settings);

  loading = true;
  public appdata: string;
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

  private fullscreen = false;
  private defaultPath = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2\\tf\\custom';

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
      this.settings = JSON.parse(data) as Settings;
    }

    if (this.settings.customPath !== null) {
      if (this.electron.fs.existsSync(this.settings.customPath)) {
        this.router.navigate(['dashboard/hud']);
      } else {
        if (this.electron.fs.existsSync(this.defaultPath)) {
          this.settings.customPath = this.defaultPath;
          this.router.navigate(['dashboard/hud']);
        } else {
          this.router.navigate(['setup']);
        }
      }
    } else {
      if (this.electron.fs.existsSync(this.defaultPath)) {
        this.settings.customPath = this.defaultPath;
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

  ngOnInit() {
    this.loading = false;
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

    // Find paths in customDir
    customDir.forEach(file => {

      // try to find huds
      if (what === 'huds' || what === null) {
        if (file.endsWith('info.vdf')) {
          const hudPath = file.split('\\');
          hudPath.pop();
          this.huds.push(hudPath.join('\\'));
        }
      }

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
      }

      // try to find weapon sounds
      if (what === 'weaponSounds' || what === null) {
        if (file.endsWith('game_sounds_weapons.txt')) {
          this.weponSounds = file;
        }
      }
    });
  }

  minimize() {
    this.electron.ipcRenderer.send('minimize');
  }

  fullscreenToggle() {
    if (this.fullscreen) {
      document.exitFullscreen()
        .then(() => {
          this.fullscreenIcon = 'fullscreen';
          this.fullscreenTip = 'Maximize';
          this.fullscreen = false;
        });
    } else {
      document.documentElement.requestFullscreen()
        .then(() => {
          this.fullscreenIcon = 'fullscreen_exit';
          this.fullscreenTip = 'Restore Down';
          this.fullscreen = true;
        });
    }
  }

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
