import React, { useState } from 'react'

export default function goal_form({ props: {selectedData, goalDeleter, processForm, modalCloser, goalErrorsClearer}}) {
  const goal_categories = "Retirement Wedding College Travel Emergency Purchase General Other".split(' ')
  const { errors, formType, passedGoal, accounts } = selectedData
  const accountsList = Object.values(accounts);

  const [goal, setGoal] = useState(passedGoal)

  const update = (field) => e => setGoal({...goal, [field]: e.currentTarget.value })

  const handleSubmit = (e) => {
    e.preventDefault();
    processForm(goal).then(
      () => modalCloser()).then(
      () => goalErrorsClearer())
  };

  const handleClose = (e) => {
    e.preventDefault();
    modalCloser();
    goalErrorsClearer();
  };

  const renderErrors = () => (
    <ul className="modal-form-errors">
      {errors.map((error, i) => (
        <li className="modal-form-error" key={i}>{error}</li>
      ))}
    </ul>
  );

  const deleteOption = () => {
    if (formType === 'edit') {
      return (
        <span className='edit-delete' onClick={() => goalDeleter(goal.id)}>Delete Goal</span>
      )
    }
  };

  
  return (
    <form onSubmit={handleSubmit} className="modal-form">
      <div onClick={handleClose} className="close-x">X</div>
      <div className="modal-inputs">
        <label>Title: 
          <input type="text" value={goal.title} onChange={update('title')} /> 
        </label>
        <label>Goal Category: 
          <select value={goal.goal_category} onChange={update('goal_category')}>
            {goal_categories.map((gc, i) => (
              <option key={i} value={`${gc}`}>{gc}</option>
            ))}
          </select>
        </label>
        <label>Goal Amount:
          <input type="number" step="1" value={goal.goal_amount} onChange={update('goal_amount')}/>
        </label>
        <label>Account:
          <select value={goal.account_id} onChange={update('account_id')}>
            {accountsList.map((account, i) => (
              <option key={i} value={account.id}>{account.label}</option>
            ))}
          </select>
        </label>
            <button className="modal-form-submit" value={formType}>{formType.toUpperCase()} GOAL</button>
            {deleteOption()}
            {renderErrors()}
      </div>
    </form>
  );
}
