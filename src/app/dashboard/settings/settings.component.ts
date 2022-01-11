import { CdkTextareaAutosize } from '@angular/cdk/text-field';
import { Component, NgZone, OnDestroy, ViewChild } from '@angular/core';
import { take } from 'rxjs';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { LogService } from '../../services/log.service';
import { SnackService } from '../../services/snack.service';

/**
 * Settings Component
 */
@Component({
  selector: 'app-settings',
  templateUrl: './settings.component.html',
  styleUrls: ['./settings.component.scss']
})
export class SettingsComponent implements OnDestroy {

  /**
   * View child of autosize
   */
  @ViewChild('autosize') autosize: CdkTextareaAutosize;

  /**
   * Creates an instance of settings component.
   *
   * @param app
   * @param electron
   * @param snack
   * @param log
   * @param ngZone
   */
  constructor(
    public app: AppComponent,
    private electron: ElectronService,
    private snack: SnackService,
    private log: LogService,
    private ngZone: NgZone
  ) {
    this.log.scope('SETTINGS');
  }

  /**
   * Triggers resize of logs view
   */
  triggerResize() {
    // Wait for changes to be applied, then trigger textarea resize.
    this.ngZone.onStable.pipe(take(1)).subscribe(() => this.autosize.resizeToFitContent(true));
  }

  /**
   * Gets log
   *
   * @returns
   */
  getLog(): string {
    const logs = this.electron.fs.readFileSync(this.electron.appData('TF2Tools\\log.log'), { encoding: 'utf8', flag: 'r' });
    const logData = logs.split('\n');
    console.log(logData.length);
    return logData.reverse().join('\n');
  }

  /**
   * Gets GitHub body
   *
   * @param body
   * @returns git hub body
   */
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
          this.log.info('MOVE', `Trying to move library. "${this.app.settings.libraryPath}" => "${path}"`);
          this.electron.fs.move(this.app.settings.libraryPath, path, { overwrite: true })
            .then(() => {
              this.log.info('MOVE', `Successfully moved your library`);
              this.app.settings.libraryPath = path;
              this.app.settingsUpdate.next(this.app.settings);
              this.snack.show('Your library was moved!');
              this.app.loading = false;
            })
            .catch(err => this.log.error('MOVE', err));

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
