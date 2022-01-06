import { Component, OnDestroy } from '@angular/core';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { SnackService } from '../../services/snack.service';

/**
 * Settings
 */
@Component({
  selector: 'app-settings',
  templateUrl: './settings.component.html',
  styleUrls: ['./settings.component.scss']
})
export class SettingsComponent implements OnDestroy {

  /**
   * Creates an instance of settings component.
   *
   * @param app
   * @param electron
   * @param snack
   */
  constructor(
    public app: AppComponent,
    private electron: ElectronService,
    private snack: SnackService
  ) { }

  /**
   * Gets log
   *
   * @returns
   */
  getLog(): string[] {
    const logs = this.electron.fs.readFileSync(this.electron.appData('TF2Tools\\log.txt'), { encoding: 'utf8', flag: 'r' });
    return logs.split('\n');
  }

  getGitHubBody(body: string): string {

    const r = [];
    body
      .replace(/\r/g, '')
      .split('\n')
      .filter(line => !line.includes('by @dependabot'))
      .filter(line => !line.includes('Full Changelog'))
      .filter(line => !line.includes(`## What's Changed`))
      .filter(line => line)
      .forEach(line => {
        const temp = line.split('by @');
        r.push(temp[0]);
      });

    return r.join('\n');
  }

  /**
   * Changes library
   */
  async changeLibrary(): Promise<void> {
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
          this.app.log.next(`Trying to move your library. ${this.app.settings.libraryPath} => ${path}`);
          this.electron.fs.move(this.app.settings.libraryPath, path, { overwrite: true })
            .then(() => {
              this.app.log.next(`Successfully moved your library`);
              this.app.settings.libraryPath = path;
              this.app.settingsUpdate.next(this.app.settings);
              this.snack.show('Your library was moved!');
              this.app.loading = false;
            })
            .catch(err => this.app.error(err));

        } else {
          this.app.loading = false;
          this.app.log.next(`Failed to moved your library`);
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

  /**
   * Updates settings
   */
  updateSettings(): void {
    this.app.settingsUpdate.next(this.app.settings);
  }

  /**
   * on destroy
   */
  ngOnDestroy(): void {
    window.removeAllListeners();
  }
}
