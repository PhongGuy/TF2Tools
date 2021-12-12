import { Component, OnInit } from '@angular/core';
import { Router } from '@angular/router';
import { ElectronService } from './core/services';
import { Paths } from './models/paths';
import { Settings } from './models/settings';
import { FileHelpService } from './services/file-help.service';
import { SnackService } from './services/snack.service';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit {

  public path = new Paths();
  public settings = new Settings();

  loading = true;
  public appdata: string;
  public appTemp: string;
  fullscreenIcon = 'fullscreen';
  fullscreenTip = 'Maximize';

  private fullscreen = false;
  private defaultPath = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2';

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
  }

  /**
   * When we want to add mastercomfig, this is the url to the latest relese
   * https://github.com/mastercomfig/mastercomfig/releases/latest/download/mastercomfig-low-preset.vpk
   */

  ngOnInit() {
    this.update(null, true);
  }

  async update(what: 'huds' | 'hitsounds' | 'vtf' | 'weaponSounds' | null = null, route = false) {

    // reset
    if (what === 'huds') {
      this.path.huds = [];
    } else if (what === 'hitsounds') {
      this.path.hitsounds = [];
      this.path.killsounds = [];
    } else if (what === 'vtf') {
      this.path.vtf = [];
    } else if (what === 'weaponSounds') {
      this.path.weponSounds = null;
    } else {
      this.path = new Paths();
    }

    // check where custom folder is
    if (this.electron.fs.existsSync(this.defaultPath)) {
      this.path.custom = `${this.defaultPath}/tf/custom`;
      const customDir = this.fileHelp.getAllFiles(this.path.custom);


      // Find paths in customDir
      customDir.forEach(file => {

        // try to find huds
        if (what === 'huds' || what === null) {
          if (file.endsWith('info.vdf')) {
            const hudPath = file.split('\\');
            hudPath.pop();
            this.path.huds.push(hudPath.join('\\'));
          }
        }

        // try to find hitsounds
        if (what === 'hitsounds' || what === null) {
          if (file.endsWith('hitsound.wav')) {
            this.path.hitsounds.push(file);
          }

          // try to find killsounds
          if (file.endsWith('killsound.wav')) {
            this.path.killsounds.push(file);
          }
        }

        // try to find vtf crosshairs
        if (what === 'vtf' || what === null) {
          if (file.includes('materials\\vgui\\replay\\thumbnails') && file.endsWith('.vtf')) {
            let err = 0;
            for (const hud of this.path.huds) {
              if (file.startsWith(hud)) {
                err++;
              }
            }
            if (err === 0) {
              this.path.vtf.push(file);
            }
          }
        }

        // try to find weapon sounds
        if (what === 'weaponSounds' || what === null) {
          if (file.endsWith('game_sounds_weapons.txt')) {
            this.path.weponSounds = file;
          }
        }
      });
      console.log(this.path);

      if (route) {
        this.router.navigate(['dashboard/hud']);
      }
    } else {
      // if we can't find it say no supported
      this.snack.show('We dont support this platform yet', null, 9000);
    }
    this.loading = false;

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
