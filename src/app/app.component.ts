import { HttpClient } from '@angular/common/http';
import { Component, OnInit } from '@angular/core';
import { Router } from '@angular/router';
import { TranslateService } from '@ngx-translate/core';
import { ElectronService } from './core/services';
import { HudList } from './models/hudList';

@Component({
  selector: 'body',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit {

  private defaultPath: string = 'C:/Program Files (x86)/Steam/steamapps/common/Team Fortress 2/tf';
  public tfPath: string = '';
  public customPath: string = '';
  public appdata: string;
  fullscreenIcon: string = 'fullscreen';
  fullscreenTip: string = 'Maximize';
  private fullscreen: boolean = false;

  public hudList: HudList[] = [];

  constructor(
    private electron: ElectronService,
    private translate: TranslateService,
    private router: Router,
    private http: HttpClient
  ) {
    this.appdata = this.electron.appData('TF2Tools');
    this.translate.setDefaultLang('en');
    if (!this.electron.fs.existsSync(this.appdata)) {
      this.electron.fs.mkdir(this.appdata);
    }
    if (this.electron.fs.existsSync(this.defaultPath)) {
      this.tfPath = this.defaultPath;
      this.customPath = this.defaultPath + '/custom'
      this.router.navigate(['dashboard/hud']);
    } else {
      this.router.navigate(['setup']);
    }
  }

  ngOnInit() {
    // this.downloadHudList();
  }

  downloadHudList() {
    const list = this.electron.fs.createWriteStream("list.md");
    this.electron.https.get("https://raw.githubusercontent.com/Hypnootize/TF2-HUDs-Megalist/master/Active%20Huds%20List.md", (response) => {
      response.pipe(list)
        .on('close', () => {
          this.electron.fs.readFile('list.md', { encoding: 'utf8', flag: 'r' })
            .then((f) => {
              const h = f.split('--------	|	------------------------	|	-------	|	----------	|	-------	|	-----------	|	--------------	|	-------	|	---------------')[1].split('\n');

              h.forEach(m => {
                const data = m.split('	|	');
                if (data[0] !== '') {
                  const creatorMaintainer = data[1].split(' ');

                  const item = new HudList();
                  item.name = data[0];
                  item.creator = creatorMaintainer[0].split('`').join('')
                  item.maintainer = creatorMaintainer[1] ? creatorMaintainer[1].split('*').join('') : '';
                  item.screenshots = data[2];
                  item.repository = data[3];
                  item.page = data[4];
                  item.group = data[5];
                  item.discussion = data[6];
                  item.discord = data[7]
                  item.download = data[8];
                  this.hudList.push(item);
                }
              });
            });
        });
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
    window.close();
  }
}
