import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { FormControl } from '@angular/forms';
import { MatDialog } from '@angular/material/dialog';
import { map, Observable, startWith } from 'rxjs';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { QuestionAnswerComponent } from '../../dialogs/question-answer/question-answer.component';
import { YesNoComponent } from '../../dialogs/yes-no/yes-no.component';
import { Hitsound } from '../../models/hitsound';
import { QuestionAnswer } from '../../models/questionAnswer';
import { YesNo } from '../../models/yesNo';
import { SnackService } from '../../services/snack.service';
import { MultipleWarningComponent } from './multiple-warning/multiple-warning.component';

@Component({
  selector: 'app-hitsound',
  templateUrl: './hitsound.component.html',
  styleUrls: ['./hitsound.component.scss']
})
export class HitsoundComponent implements OnInit {

  @ViewChild('folderUpload') folderUpload: ElementRef;

  localHitsounds: string;
  library: Hitsound[];
  hitsounds: Hitsound[];

  myControl = new FormControl();
  filteredOptions: Observable<Hitsound[]>;

  constructor(
    private electron: ElectronService,
    public app: AppComponent,
    private dialog: MatDialog,
    private snack: SnackService
  ) {
    this.localHitsounds = this.electron.appData('TF2Tools/hitsounds');
  }

  ngOnInit(): void {
    // update info in case user changed shit when TF2Tools was open
    this.app.update('hitsounds');

    // check if there is multiple hitsounds installed and warn user
    if (this.app.hitsounds.length > 1) {
      this.dialog.open(MultipleWarningComponent, {
        width: '450px',
        data: this.app.hitsounds
      });
    }

    this.update();

    this.filteredOptions = this.myControl.valueChanges.pipe(
      startWith(''),
      map(value => this.hitsoundFilter(value)),
    );
  }

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

  upload(event: Event) {
    const target = event.target as HTMLInputElement;
    const files: FileList = target.files;
    if (files.length === 1 && files[0].name.split('.').pop() === 'wav') {
      const name = files[0].name;
      const path = files[0].path;
      const dest = `${this.localHitsounds}\\${name}`;
      if (this.electron.fs.existsSync(dest)) {
        this.snack.show(`There is already a sound by that name in the library`);
      } else {
        if (name === 'hitsound.wav') {
          const dialogRef = this.dialog.open(QuestionAnswerComponent, {
            width: '450px',
            data: {
              question: 'Give the sound a name',
              subQuestion: '',
              cant: this.getNames(null)
            } as QuestionAnswer
          });

          dialogRef.afterClosed().subscribe(r => {
            if (typeof r === 'string') {
              this.electron.fs.copy(path, `${this.localHitsounds}\\${r}.wav`)
                .then(() => {
                  this.snack.show(`${name} was added`);
                  this.update();
                })
                .catch(err => this.app.error(err));
            }
          });
        } else {
          this.electron.fs.copy(path, dest)
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

  rename(_hitsound: Hitsound) {
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

  remove(_hitsound: Hitsound) {

    const info = new YesNo();
    info.question = `Remove ${_hitsound.name}?`;
    info.subQuestion = `Are you sure you want to remove ${_hitsound.name}? This cannot be undone!`;

    const dialogRef = this.dialog.open(YesNoComponent, {
      width: '450px',
      data: info
    });

    dialogRef.afterClosed().subscribe(result => {
      if (result) {
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

  removeFile(file: string, _name: string): void {
    this.remove({ name: _name, path: file });
  }

  playFile(file: string): void {
    this.play({ name: 'hitsound', path: file });
  }

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

  volumeChane() {
    this.app.settingsUpdate.next(this.app.settings);
  }

  installHitsound(_hitsound: Hitsound): void {
    const path = this.app.hitsounds[0];
    if (path !== undefined) {
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

  installkillsound(_killsound: Hitsound): void {
    const path = this.app.killsounds[0];
    if (path !== undefined) {
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

  private hitsoundFilter(value: string): Hitsound[] {
    const filterValue = value.toLowerCase();

    return this.library.filter(option => option.name.toLowerCase().includes(filterValue));
  }

  private getNames(exclude: string): string[] {
    const names: string[] = [];
    this.library.forEach(a => {
      if (a.name !== exclude) {
        names.push(a.name);
      }
    });
    return names;
  }

}
