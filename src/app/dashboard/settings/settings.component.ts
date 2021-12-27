import { HttpClient } from '@angular/common/http';
import { Component, OnDestroy, OnInit } from '@angular/core';
import { MatDialog } from '@angular/material/dialog';
import { APP_CONFIG } from '../../../environments/environment';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { SnackService } from '../../services/snack.service';

@Component({
  selector: 'app-settings',
  templateUrl: './settings.component.html',
  styleUrls: ['./settings.component.scss']
})
export class SettingsComponent implements OnInit, OnDestroy {

  update = false;

  constructor(
    public app: AppComponent,
    private electron: ElectronService,
    private snack: SnackService,
    private dialog: MatDialog,
    private http: HttpClient
  ) { }

  ngOnInit(): void {
    this.http.get('https://api.github.com/repos/PhongGuy/TF2Tools/releases/latest').subscribe((json: any) => {
      if (APP_CONFIG.version !== json.tag_name.replace('v', '')) {
        this.update = true;
      }
    });
  }

  async changeLibrary() {
    this.app.loading = true;
    this.electron.ipcRenderer.send('openDialog', {
      title: 'Chose location',
      defaultPath: this.app.settings.libraryPath,
      properties: ['openDirectory']
    } as Electron.OpenDialogOptions);

    this.electron.ipcRenderer.once('openDialogResponse', (event, args: { canceled: boolean; filePaths: string[] }) => {
      if (!args.canceled) {
        let path = args.filePaths[0];

        if (this.electron.fs.existsSync(path)) {

          path = `${path}\\TF2Tools\\Library`;
          this.electron.fs.ensureDirSync(path);
          this.snack.show('Moving your library. This can take some time if you have a big library.', null, 6000);
          this.electron.fs.move(this.app.settings.libraryPath, path, { overwrite: true })
            .then(() => {
              this.app.settings.libraryPath = path;
              this.app.settingsUpdate.next(this.app.settings);
              this.snack.show('Your library was moved!');
              this.app.loading = false;
            })
            .catch(err => this.app.error(err));

        } else {
          this.app.loading = false;
          this.snack.show('That is not a location we can use');
        }
      } else {
        this.app.loading = false;
      }
    });

    window.addEventListener('focus', e => {
      e.preventDefault();
      setTimeout(() => {
        window.focus();
        window.removeAllListeners();
      }, 75);
    });
  }

  updateSettings() {
    this.app.settingsUpdate.next(this.app.settings);
  }

  ngOnDestroy(): void {
    window.removeAllListeners();
  }
}
