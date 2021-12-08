import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { FormControl } from '@angular/forms';
import { MatDialog } from '@angular/material/dialog';
import { map, Observable, startWith } from 'rxjs';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { QuestionAnswerComponent } from '../../dialogs/question-answer/question-answer.component';
import { YesNoComponent } from '../../dialogs/yes-no/yes-no.component';
import { Hud } from '../../models/hud';
import { QuestionAnswer } from '../../models/questionAnswer';
import { SnackService } from '../../services/snack.service';

@Component({
  selector: 'app-hud',
  templateUrl: './hud.component.html',
  styleUrls: ['./hud.component.scss']
})
export class HudComponent implements OnInit {

  currentHuds: Hud[];
  hudsInstalled: number;
  library: Hud[];
  localHuds: string;
  notInLibrary: Hud[];

  @ViewChild('folderUpload') folderUpload: ElementRef;

  myControl = new FormControl();
  filteredOptions: Observable<Hud[]>;

  constructor(
    private electron: ElectronService,
    private app: AppComponent,
    public dialog: MatDialog,
    private snack: SnackService
  ) {
    this.localHuds = `${this.app.appdata}\\huds`;
  }

  // first we need to look for a folder with info file in, we will assume that is the current hud.
  ngOnInit(): void {
    this.update();

    this.filteredOptions = this.myControl.valueChanges.pipe(
      startWith(''),
      map(value => this._filter(value)),
    );
  }

  private _filter(value: string): Hud[] {
    const filterValue = value.toLowerCase();

    return this.library.filter(option => {
      if (option.folderName.toLowerCase().includes(filterValue)) {
        return true;
      } else if (option.name.toLowerCase().includes(filterValue)) {
        return true;
      }
    });
  }

  async update() {
    this.library = [];
    if (!this.electron.fs.existsSync(this.localHuds)) {
      this.electron.fs.mkdirSync(this.localHuds);
    }

    this.electron.fs.readdirSync(this.localHuds).forEach(customFiles => {
      let info = this.localHuds + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        const data = this.electron.fs.readFileSync(info, { encoding: 'utf8', flag: 'r' }).split('"')
        const h = { name: data[1], folderName: customFiles, version: data[5], path: this.localHuds + '\\' + customFiles };
        this.library.push(h);
      }
    });

    this.hudsInstalled = 0;
    this.currentHuds = [];
    this.notInLibrary = [];
    this.electron.fs.readdirSync(this.app.customPath).forEach(customFiles => {
      let info = this.app.customPath + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        this.hudsInstalled++;

        const data = this.electron.fs.readFileSync(info, { encoding: 'utf8', flag: 'r' }).split('"');
        const h = { name: data[1], folderName: customFiles, version: data[5], path: this.app.customPath + '\\' + customFiles };
        const l = { name: data[1], folderName: customFiles, version: data[5], path: this.localHuds + '\\' + customFiles };

        this.currentHuds.push(h);

        if (!this.electron.fs.existsSync(l.path)) {
          this.notInLibrary.push(h);
        }
      }
    });

    this.myControl.setValue('');
  }

  add(_hud: Hud) {
    this.electron.fs.copy(_hud.path, `${this.app.customPath}/${_hud.folderName}`)
      .then(() => {
        this.snack.show(`${_hud.folderName} was installed`);
        this.update();
      })
      .catch(err => console.error(err));
  }

  uninstall(_hud: Hud, warning: boolean) {
    if (this.electron.fs.existsSync(_hud.path)) {

      if (warning) {

        const dialogRef = this.dialog.open(YesNoComponent, {
          width: '450px',
          data: { question: `Remove ${_hud.folderName}?`, subquestion: `Are you sure you want to remove ${_hud.folderName}? This cannot be undone!` }
        });

        dialogRef.afterClosed().subscribe(result => {
          if (result) {
            this.electron.fs.remove(_hud.path)
              .then(() => {
                this.snack.show(`${_hud.folderName} was removed`)
                this.update();
              });
          }
        });
      } else {
        this.electron.fs.remove(_hud.path)
          .then(() => {
            this.snack.show(`${_hud.folderName} was uninstalled`)
            this.update();
          });
      }
    } else {
      console.log(false)
    }
  }

  upload(event: Event) {
    const files: File[] = Array.from(event.target['files']);
    const hudsFound = files.filter(i => i.name == 'info.vdf');
    if (hudsFound.length > 0) {
      hudsFound.forEach(hud => {
        const path = hud.path.replace('\\info.vdf', '');
        const name = path.split('\\').pop();
        const dest = `${this.localHuds}\\${name}`

        if (!this.electron.fs.existsSync(dest)) {
          this.electron.fs.copy(path, dest)
            .then(() => {
              this.snack.show(`Added "${name}" to library`, null, hudsFound.length > 2 ? 1000 : 3000);
              const data = this.electron.fs.readFileSync(hud.path, { encoding: 'utf8', flag: 'r' }).split('"');
              const l = { name: data[1], folderName: name, version: data[5], path: this.localHuds + '\\' + name };
              this.library.push(l);
              this.myControl.setValue('');
            }).catch(err => console.error(err));
        } else {
          this.snack.show(`${name} is already in library`);
        }
      });
    } else {
      this.snack.show('We could not find any hud here. Are you sure you selected the right folder and it has the info.vdf file inside.', null, 6000)
    }

    this.folderUpload.nativeElement.value = null;
  }

  rename(_hud: Hud): void {
    const dialogRef = this.dialog.open(QuestionAnswerComponent, {
      width: '450px',
      data: {
        question: `Rename ${_hud.folderName}`,
        subQuestion: '',
        cant: this.getNames(_hud.folderName)
      } as QuestionAnswer
    });

    dialogRef.afterClosed().subscribe(result => {
      if (typeof result === 'string') {
        const path = _hud.path.split('\\');
        const name = path.pop();
        this.electron.fs.rename(_hud.path, `${path.join('\\')}\\${result}`)
          .then(() => {
            this.snack.show(`Updated "${name}" to "${result}"`);
            this.update();
          })
          .catch(err => console.error(err));
      }
    });
  }

  private getNames(exclude: string): string[] {
    const names: string[] = [];
    this.library.forEach(a => {
      if (a.folderName !== exclude) {
        names.push(a.folderName);
      }
    })
    return names;
  }

  replace(_hud: Hud): void {

  }

  private hudCheck(files: FileList): boolean {
    var check: boolean = false;
    if (files.length > 0) {
      for (let i = 0; i < files.length; i++) {
        if (files[i].name == 'info.vdf') {
          check = true;
          break;
        }
      }
    }
    return check;
  }

  private copy(path, dest) {
    this.electron.fs.copy(path, dest)
      .then(() => this.update())
      .catch(err => console.error(err));
  }
}
