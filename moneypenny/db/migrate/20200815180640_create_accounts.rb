class CreateAccounts < ActiveRecord::Migration[5.2]
  def change
    create_table :accounts do |t|
      t.boolean :debit, null: false
      t.string :account_category, null: false 
      t.string :institution, null: false 
      t.string :label, null: false
      t.float :balance, null: false 
      t.integer :user_id, null: false
      t.timestamps
    end
    add_index :accounts, :user_id
  end
end
