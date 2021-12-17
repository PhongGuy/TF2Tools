import { Component, Inject, OnInit } from '@angular/core';
import { FormControl, FormGroupDirective, NgForm, Validators } from '@angular/forms';
import { ErrorStateMatcher } from '@angular/material/core';
import { MatDialogRef, MAT_DIALOG_DATA } from '@angular/material/dialog';
import { UploadChangeName } from '../../../models/uploadChangeName';
import { ValidatorService } from '../../../services/validators.service';

/** Error when invalid control is dirty, touched, or submitted. */
export class MyErrorStateMatcher implements ErrorStateMatcher {
  isErrorState(control: FormControl | null, form: FormGroupDirective | NgForm | null): boolean {
    const isSubmitted = form && form.submitted;
    return !!(control && control.invalid && (control.dirty || control.touched || isSubmitted));
  }
}

@Component({
  selector: 'app-upload-change-name',
  templateUrl: './upload-change-name.component.html',
  styleUrls: ['./upload-change-name.component.scss']
})
export class UploadChangeNameComponent implements OnInit {

  input: FormControl;
  matcher: MyErrorStateMatcher;

  constructor(
    public dialogRef: MatDialogRef<UploadChangeNameComponent>,
    @Inject(MAT_DIALOG_DATA) public data: UploadChangeName,
    private validator: ValidatorService
  ) { }

  ngOnInit(): void {
    this.matcher = new MyErrorStateMatcher();
    this.input = new FormControl('', [Validators.required, this.validator.checkName(this.data.cant), this.validator.special()]);
  }

  submit() {
    if (!this.input.invalid) {
      this.dialogRef.close(this.input.value);
    }
  }

  playFile(event: Event, file: File) {
    event.preventDefault();
    const audio = new Audio(file.path);
    audio.volume = this.data.volume / 100;
    audio.load();
    audio.play();
  }
}
