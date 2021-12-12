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
  volume = 10;

  myControl = new FormControl();
  filteredOptions: Observable<Hitsound[]>;

  constructor(
    private electron: ElectronService,
    private app: AppComponent,
    private dialog: MatDialog,
    private snack: SnackService
  ) {
    this.localHitsounds = this.electron.appData('TF2Tools/hitsounds');
  }

  ngOnInit(): void {
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

    // console.log(this.file.getAllFiles(this.app.customPath))


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
                });
            }
          });
        } else {
          this.electron.fs.copy(path, dest)
            .then(() => {
              this.snack.show(`${name} was added`);
              this.update();
            });
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
            });
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
            this.update();
          });
      }
    });
  }

  play(_hitsound: Hitsound) {
    const audio = new Audio(_hitsound.path);
    audio.volume = this.volume / 100;
    audio.load();
    audio.play();
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
