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
  loading = true;
  public appdata: string;
  fullscreenIcon = 'fullscreen';
  fullscreenTip = 'Maximize';
  public settings: Settings = new Settings();

  private fullscreen = false;
  private defaultPath = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2';

  constructor(
    private electron: ElectronService,
    private router: Router,
    private snack: SnackService,
    private fileHelp: FileHelpService
  ) {
    this.appdata = this.electron.appData('TF2Tools');
    this.electron.fs.ensureDir(this.appdata);
  }

  /**
   * When we want to add mastercomfig, this is the url to the latest relese
   * https://github.com/mastercomfig/mastercomfig/releases/latest/download/mastercomfig-low-preset.vpk
   */

  ngOnInit() {
    this.update(true);
  }

  update(route = false) {

    // check where custom folder is
    if (this.electron.fs.existsSync(this.defaultPath)) {
      this.path.custom = `${this.defaultPath}/tf/custom`;
      const customDir = this.fileHelp.getAllFiles(this.path.custom);

      // Find paths in customDir
      customDir.forEach(file => {

        // try to find huds
        if (file.endsWith('info.vdf')) {
          const hudPath = file.split('\\');
          hudPath.pop();
          this.path.huds.push(hudPath.join('\\'));
        }

        // try to find hitsounds
        if (file.endsWith('hitsound.wav')) {
          this.path.hitsounds.push(file);
        }

        // try to find killsounds
        if (file.endsWith('killsound.wav')) {
          this.path.killsounds.push(file);
        }

        // try to find vtf crosshairs
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

        // try to find weapon sounds
        if (file.endsWith('game_sounds_weapons.txt')) {
          this.path.weponSounds = file;
        }
      });

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
}
