import { Component, Inject, OnInit } from '@angular/core';
import { AbstractControl, FormControl, FormGroupDirective, NgForm, ValidatorFn, Validators } from '@angular/forms';
import { ErrorStateMatcher } from '@angular/material/core';
import { MatDialogRef, MAT_DIALOG_DATA } from '@angular/material/dialog';
import { QuestionAnswer } from '../../models/questionAnswer';

/** Error when invalid control is dirty, touched, or submitted. */
export class MyErrorStateMatcher implements ErrorStateMatcher {
  isErrorState(control: FormControl | null, form: FormGroupDirective | NgForm | null): boolean {
    const isSubmitted = form && form.submitted;
    return !!(control && control.invalid && (control.dirty || control.touched || isSubmitted));
  }
}

@Component({
  selector: 'app-question-answer',
  templateUrl: './question-answer.component.html',
  styleUrls: ['./question-answer.component.scss']
})
export class QuestionAnswerComponent implements OnInit {

  input: FormControl;
  matcher: MyErrorStateMatcher;

  constructor(
    public dialogRef: MatDialogRef<QuestionAnswerComponent>,
    @Inject(MAT_DIALOG_DATA) public data: QuestionAnswer) { }

  ngOnInit(): void {
    this.matcher = new MyErrorStateMatcher();
    this.input = new FormControl('', [Validators.required, this.checkName(this.data.cant)]);
  }

  checkName(cant: string[]): ValidatorFn {
    return (control: AbstractControl): { [key: string]: boolean } | null => {
      if (cant.filter(a => a == control.value).length !== 0) {
        return { nameExists: true }
      }
      return null
    }
  }

  submit() {
    if (!this.input.invalid) {
      this.dialogRef.close(this.input.value);
    }
  }
}
