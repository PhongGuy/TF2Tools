import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { FormControl } from '@angular/forms';
import { MatDialog } from '@angular/material/dialog';
import { firstValueFrom, map, Observable, startWith } from 'rxjs';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { QuestionAnswerComponent } from '../../dialogs/question-answer/question-answer.component';
import { YesNoComponent } from '../../dialogs/yes-no/yes-no.component';
import { Hitsound } from '../../models/hitsound';
import { QuestionAnswer } from '../../models/questionAnswer';
import { UploadChangeName } from '../../models/uploadChangeName';
import { YesNo } from '../../models/yesNo';
import { SnackService } from '../../services/snack.service';
import { MultipleWarningComponent } from './multiple-warning/multiple-warning.component';
import { UploadChangeNameComponent } from './upload-change-name/upload-change-name.component';

/**
 * Hitsound component
 */
@Component({
  selector: 'app-hitsound',
  templateUrl: './hitsound.component.html'
})
export class HitsoundComponent implements OnInit {

  /**
   * folder upload `ElementRef`
   */
  @ViewChild('folderUpload') folderUpload: ElementRef;

  /**
   * Local hitsounds of hitsound component
   */
  localHitsounds: string;
  /**
   * Library  of hitsound component
   */
  library: Hitsound[];
  /**
   * Hitsounds  of hitsound component
   */
  hitsounds: Hitsound[];

  /**
   * My control of hitsound component
   */
  myControl = new FormControl();
  /**
   * Filtered options of hitsound component
   */
  filteredOptions: Observable<Hitsound[]>;

  /**
   * Creates an instance of hitsound component.
   *
   * @param electron
   * @param app AppComponent
   * @param dialog
   * @param snack
   */
  constructor(
    private electron: ElectronService,
    public app: AppComponent,
    private dialog: MatDialog,
    private snack: SnackService
  ) {
    this.localHitsounds = `${this.app.settings.libraryPath}\\hitsounds`;
    this.electron.fs.ensureDir(this.localHitsounds);
  }

  /**
   * on init
   */
  ngOnInit(): void {
    // update info in case user changed shit when TF2Tools was open
    this.app.update('hitsounds');
    this.update();

    // check if there is multiple hitsounds installed and warn user
    if (this.app.hitsounds.length > 1) {
      this.dialog.open(MultipleWarningComponent, {
        width: '450px',
        data: this.app.hitsounds
      });
    }

    this.filteredOptions = this.myControl.valueChanges.pipe(
      startWith(''),
      map(value => this.hitsoundFilter(value)),
    );
  }

  /**
   * Updates hitsound component
   */
  async update() {
    this.library = [];
    this.electron.fs.readdirSync(this.localHitsounds).forEach(file => {
      const name = file.split('.');
      name.pop();
      const h = { name: name.toString(), path: `${this.localHitsounds}\\${name}.wav` };
      this.library.push(h);
    });
    this.myControl.setValue('');
  }

  /**
   * Uploads hitsound component
   *
   * @param event
   */
  async upload(event: Event) {
    const target = event.target as HTMLInputElement;
    const files: File[] = Array.from(target.files);

    for (const sound of files) {
      if (sound.name.includes('.wav')) {
        const name = sound.name.replace('.wav', '');
        const dest = `${this.localHitsounds}\\${sound.name}`;
        const exist = this.electron.fs.existsSync(dest);
        await this.update();

        if (name === 'hitsound' || name === 'killsound' || exist) {
          const dialogRef = this.dialog.open(UploadChangeNameComponent, {
            width: '450px',
            data: {
              title: exist ? 'We found a sound by that name' : 'Give the sound a name',
              cant: this.getNames(),
              file: sound,
              volume: this.app.settings.volume
            } as UploadChangeName
          });

          await firstValueFrom(dialogRef.afterClosed())
            .then((newName) => {
              if (typeof newName === 'string') {
                this.electron.fs.copy(sound.path, `${this.localHitsounds}\\${newName}.wav`)
                  .then(() => {
                    this.snack.show(`${name} was added`);
                    this.update();
                  })
                  .catch(err => this.app.error(err));
              }
            });
        } else {
          this.electron.fs.copy(sound.path, dest)
            .then(() => {
              this.snack.show(`${name} was added`);
              this.update();
            })
            .catch(err => this.app.error(err));
        }

      }
    }

    this.folderUpload.nativeElement.value = null;
  }

  /**
   * Renames hitsound
   *
   * @param _hitsound
   */
  rename(_hitsound: Hitsound): void {
    if (this.electron.fs.existsSync(_hitsound.path)) {

      const dialogRef = this.dialog.open(QuestionAnswerComponent, {
        width: '450px',
        data: {
          question: `Rename ${_hitsound.name}`,
          subQuestion: '',
          cant: this.getNames(_hitsound.name)
        } as QuestionAnswer
      });

      dialogRef.afterClosed().subscribe(r => {
        if (typeof r === 'string') {
          const path = _hitsound.path.split('\\');
          path.pop();
          this.app.log.next(`Renaming hitsound: *RENAME* "${_hitsound.path}" => "${path.join('\\')}\\${r}.wav"`);
          this.electron.fs.rename(_hitsound.path, `${path.join('\\')}\\${r}.wav`)
            .then(() => {
              this.snack.show(`Renamed "${_hitsound.name}" to "${r}"`);
              this.update();
            })
            .catch(err => this.app.error(err));
        }
      });

    } else {
      this.snack.show(`${_hitsound.name} could not be found?`);
    }
  }

  /**
   * Removes hitsound
   *
   * @param _hitsound
   */
  remove(_hitsound: Hitsound): void {

    const info = new YesNo();
    info.question = `Remove ${_hitsound.name}?`;
    info.subQuestion = `Are you sure you want to remove ${_hitsound.name}? This cannot be undone!`;

    const dialogRef = this.dialog.open(YesNoComponent, {
      width: '450px',
      data: info
    });

    dialogRef.afterClosed().subscribe(result => {
      if (result) {
        this.app.log.next(`Remove hitsound *DELETE* "${_hitsound.path}"`);
        this.electron.fs.remove(_hitsound.path)
          .then(() => {
            this.snack.show(`${_hitsound.name} was removed`);
            if (_hitsound.name === 'hitsound' || _hitsound.name === 'killsound') {
              this.app.update('hitsounds');
            } else {
              this.update();
            }
          })
          .catch(err => this.app.error(err));
      }
    });
  }

  /**
   * Removes file
   *
   * @param file
   * @param _name
   */
  removeFile(file: string, _name: string): void {
    this.remove({ name: _name, path: file });
  }

  /**
   * Plays file
   *
   * @param file
   */
  playFile(file: string): void {
    this.play({ name: 'hitsound', path: file });
  }

  /**
   * Plays hitsound
   *
   * @param _hitsound
   */
  play(_hitsound: Hitsound): void {

    if (_hitsound.name === 'hitsound') {
      const p = _hitsound.path;
      _hitsound.path = `${this.app.appTemp}\\${this.app.generateRandomString(8)}.wav`;
      this.electron.fs.copySync(p, _hitsound.path);
    }

    const audio = new Audio(_hitsound.path);
    audio.volume = this.app.settings.volume / 100;
    audio.load();
    audio.play();

    if (_hitsound.name === 'hitsound') {
      audio.addEventListener('ended', end => {
        this.electron.fs.removeSync(_hitsound.path);
      });
    }
  }

  /**
   * Volumes change
   */
  volumeChange(): void {
    this.app.settingsUpdate.next(this.app.settings);
  }

  /**
   * Installs hitsound
   *
   * @param _hitsound
   */
  installHitsound(_hitsound: Hitsound): void {
    const path = this.app.hitsounds[0];
    if (path !== undefined) {
      this.app.log.next(`Installing hitsound: *COPY* "${_hitsound.path}" => "${path}"`);
      this.electron.fs.copy(_hitsound.path, path, { overwrite: true })
        .then(() => {
          this.snack.show(`Hitsound ${_hitsound.name} was installed`);
          this.app.update('hitsounds');
        })
        .catch(err => {
          this.app.error(err);
        });
    } else {
      const defaultPath = `${this.app.settings.customPath}\\mycustomstuff\\sound\\ui`;
      this.electron.fs.ensureDir(defaultPath)
        .then(() => {
          this.app.log.next(`Installing hitsound: *COPY* "${_hitsound.path}" => "${defaultPath}\\hitsound.wav"`);
          this.electron.fs.copy(_hitsound.path, `${defaultPath}\\hitsound.wav`)
            .then(() => {
              this.snack.show(`Hitsound ${_hitsound.name} was installed`);
              this.app.update('hitsounds');
            })
            .catch(err => this.app.error(err));
        })
        .catch(err => this.app.error(err));
    }
  }

  /**
   * Installs kill sound
   *
   * @param _killsound
   */
  installKillSound(_killsound: Hitsound): void {
    const path = this.app.killsounds[0];
    if (path !== undefined) {
      this.app.log.next(`Installing killsound: *COPY* "${_killsound.path}" => "${path}"`);
      this.electron.fs.copy(_killsound.path, path, { overwrite: true })
        .then(() => {
          this.snack.show(`Killsound ${_killsound.name} was installed`);
          this.app.update('hitsounds');
        })
        .catch(err => this.app.error(err));
    } else {
      const defaultPath = `${this.app.settings.customPath}\\mycustomstuff\\sound\\ui`;
      this.electron.fs.ensureDir(defaultPath)
        .then(() => {
          this.app.log.next(`Installing killsound: *COPY* "${_killsound.path}" => "${defaultPath}\\killsound.wav"`);
          this.electron.fs.copy(_killsound.path, `${defaultPath}\\killsound.wav`)
            .then(() => {
              this.snack.show(`Killsound ${_killsound.name} was installed`);
              this.app.update('hitsounds');
            })
            .catch(err => this.app.error(err));
        })
        .catch(err => this.app.error(err));
    }
  }

  /**
   * Hitsounds filter
   *
   * @param value
   * @returns filter
   */
  private hitsoundFilter(value: string): Hitsound[] {
    const filterValue = value.toLowerCase();

    return this.library.filter(option => option.name.toLowerCase().includes(filterValue));
  }

  /**
   * Gets names
   *
   * @param [exclude]
   * @returns names
   */
  private getNames(exclude: string = null): string[] {
    const names: string[] = [];
    this.library.forEach(a => {
      if (a.name !== exclude) {
        names.push(a.name);
      }
    });
    return names;
  }

}
