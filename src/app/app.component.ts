import { Component, OnInit } from '@angular/core';
import { Router } from '@angular/router';
import { TranslateService } from '@ngx-translate/core';
import { ElectronService } from './core/services';
import { HudList } from './models/hudList';
import { Settings } from './models/settings';
import { SnackService } from './services/snack.service';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit {

  public tfPath = '';
  public customPath = '';
  public appdata: string;
  fullscreenIcon = 'fullscreen';
  fullscreenTip = 'Maximize';
  public settings: Settings = new Settings();
  public hudList: HudList[] = [];

  private fullscreen = false;
  private defaultPath = 'C:/Program Files (x86)/Steam/steamapps/common/Team Fortress 2/tf';

  constructor(
    private electron: ElectronService,
    private translate: TranslateService,
    private router: Router,
    private snack: SnackService
  ) {
    this.appdata = this.electron.appData('TF2Tools');
    this.translate.setDefaultLang('en');

    if (!this.electron.fs.existsSync(this.appdata)) {
      this.electron.fs.mkdir(this.appdata);
    }


    if (this.electron.fs.existsSync(this.defaultPath)) {
      this.tfPath = this.defaultPath;
      this.customPath = this.defaultPath + '/custom';
      this.router.navigate(['dashboard/hud']);
    } else {
      this.snack.show(`We do not support your system yet, please give me time :)`);
    }

  }

  /**
   * When we want to add mastercomfig, this is the url to the latest relese
   * https://github.com/mastercomfig/mastercomfig/releases/latest/download/mastercomfig-low-preset.vpk
   */

  ngOnInit() {
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
