class AddInstit < ActiveRecord::Migration[5.2]
  def change
    add_column :accounts, :inst, :string
    
  end
end
