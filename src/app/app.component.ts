import { HttpClient } from '@angular/common/http';
import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import * as moment from 'moment';
import { BehaviorSubject } from 'rxjs';
import { APP_CONFIG } from '../environments/environment';
import { ElectronService } from './core/services';
import { Settings } from './models/settings';
import { FileHelpService } from './services/file-help.service';
import { SnackService } from './services/snack.service';

/**
 * This is the main app component
 */
@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})

export class AppComponent implements OnInit {
  /**
   * View child room
   *
   * @param ´ElementRef´
   */
  @ViewChild('room') room: ElementRef;

  /**
   * Settings used in app
   */
  settings = new Settings();
  /**
   * Settings update of app component
   */
  settingsUpdate: BehaviorSubject<Settings> = new BehaviorSubject<Settings>(this.settings);

  /**
   * Log of app component
   */
  log: BehaviorSubject<string> = new BehaviorSubject<string>('');

  /**
   * Loading  of app component
   *
   * @param boolean Show spinner or not
   */
  loading = true;
  /**
   * Fullscreen icon of app component
   */
  fullscreenIcon = 'fullscreen';
  /**
   * Fullscreen tooltip of app component
   */
  fullscreenTip = 'Maximize';
  /**
   * Hitsounds found
   */
  hitsounds: string[] = [];
  /**
   * Killsounds found
   */
  killsounds: string[] = [];
  /**
   * Vtf found
   */
  vtf: string[] = [];
  /**
   * Vtf scripts found
   */
  vtfScripts: string[] = [];
  /**
   * Huds found
   */
  huds: string[] = [];
  /**
   * Weapon sounds of app component
   */
  weaponSounds = '';
  /**
   * Update available of app component
   *
   * @param boolean if update is available
   */
  updateAvailable = false;
  /**
   * App temp of app component
   */
  appTemp: string;
  /**
   * Appdata  of app component
   */
  private appdata: string;
  /**
   * Fullscreen
   *
   * @param boolean if app is fullscreen
   */
  private fullscreen = false;
  /**
   * Default custom path
   */
  private defaultCustomPath = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2\\tf\\custom';
  /**
   * Default library path
   */
  private defaultLibraryPath = this.electron.appData('TF2Tools\\Library');

  /**
   * Creates an instance of app component.
   *
   * @param electron
   * @param router
   * @param snack
   * @param fileHelp
   * @param http
   */
  constructor(
    private electron: ElectronService,
    private router: Router,
    private snack: SnackService,
    private fileHelp: FileHelpService,
    private http: HttpClient
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
   * on init
   */
  ngOnInit() {
    // scroll to top when we change navigation
    this.router.events.subscribe(event => {
      if (event instanceof NavigationEnd) {
        this.room.nativeElement.scrollTop = 0;
      }
    });

    // stop loading when we loaded
    this.loading = false;

    this.log.subscribe(data => {
      if (data.length > 0) {
        const log = `${moment().format('DD-MM-YYYY HH:mm:ss')}:: ${data}\n`;
        this.electron.fs.appendFile(`${this.appdata}\\log.txt`, log).then(() => {
          console.log(log);
        });
      }
    });

    // when we update settings we write them to json file
    this.settingsUpdate.subscribe(a => {
      this.electron.fs.writeFileSync(`${this.appdata}\\settings.json`, JSON.stringify(this.settings));
    });

    this.checkUpdate();
  }

  /**
   * Update custom files
   *
   * @param [what]
   */
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
      this.weaponSounds = null;
    } else {
      this.huds = [];
      this.hitsounds = [];
      this.killsounds = [];
      this.vtf = [];
      this.weaponSounds = null;
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
          this.weaponSounds = file;
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

  /**
   * Log app errors
   *
   * @param err
   */
  error(err: any): void {
    console.error(err);
    this.snack.show(err);
    this.log.next(err.toString());
  }

  /**
   * Generates random string
   *
   * @param length
   * @returns `string`
   */
  generateRandomString(length) {
    let result = ''; let seeds;

    for (let i = 0; i < length - 1; i++) {
      //Generate seeds array, that will be the bag from where randomly select generated char
      seeds = [
        Math.floor(Math.random() * 10) + 48,
        Math.floor(Math.random() * 25) + 65,
        Math.floor(Math.random() * 25) + 97
      ];

      //Choose randomly from seeds, convert to char and append to result
      result += String.fromCharCode(seeds[Math.floor(Math.random() * 3)]);
    }

    return result;
  }

  /**
   * Checks for new release
   */
  private async checkUpdate() {
    this.http.get('https://api.github.com/repos/PhongGuy/TF2Tools/releases/latest').subscribe((json: any) => {
      if (APP_CONFIG.version !== json.tag_name.replace('v', '')) {
        this.snack.show('New version available');
        this.updateAvailable = true;
      }
    });
  }
}
